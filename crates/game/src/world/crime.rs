// SPDX-License-Identifier: GPL-3.0-or-later

//! Recording a witnessed crime. See `crate::crime` for the crime types, the
//! `FACT.CRVA` pricing table and this module's `Error`.

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Writer};
use soe_papyrus::VmError;

use crate::crime::{ArrestChoice, ArrestOptions, ArrestResult, CrimeType, Error};
use crate::inventory::Field;

use super::{World, T};

impl World {
    /// Return the choices offered for an arrest by the player's crime faction.
    ///
    /// Payment is offered only when the player has enough `Gold001`, jail is
    /// offered for a positive bounty, and resisting is always available.
    /// The faction must track crime and its `CRVA` record must enable arrest.
    ///
    /// # Errors
    /// Returns [`Error::Actor`] when the player is not a valid actor,
    /// [`Error::NoCrimeFaction`] when no crime faction is assigned, or
    /// [`Error::NotTracked`] and [`Error::ArrestDisabled`] when the faction
    /// cannot arrest the player.
    pub fn arrest_options(&mut self) -> Result<ArrestOptions, Error> {
        let player = self.player();
        let faction_id = self
            .actor(player)
            .ok_or(Error::Actor(player))?
            .crime_faction
            .ok_or(Error::NoCrimeFaction(player))?;
        let values = self.crime_values(faction_id);
        let faction = self
            .faction(faction_id)
            .ok_or(Error::NotTracked(faction_id))?;
        if !faction.tracks_crime {
            return Err(Error::NotTracked(faction_id));
        }
        if !values.arrest {
            return Err(Error::ArrestDisabled);
        }
        let bounty = faction
            .crime_gold_violent
            .saturating_add(faction.crime_gold_nonviolent)
            .max(0);
        let can_pay = if bounty > 0 {
            self.form("Gold001")
                .is_some_and(|gold| self.item_count(player, gold) >= bounty)
        } else {
            false
        };
        Ok(ArrestOptions {
            bounty,
            can_pay,
            can_serve_time: bounty > 0,
            can_resist: true,
        })
    }

    /// Resolve the selected arrest choice against the player's crime faction.
    ///
    /// A payment deducts the full bounty and clears both bounty ledgers. Jail
    /// delegates to `serve_jail_time`. Resistance leaves the purse, bounty,
    /// infamy and jail state unchanged. Invalid choices fail before mutation.
    ///
    /// # Errors
    /// Returns the validation errors from [`Self::arrest_options`],
    /// [`Error::NoBounty`] when a payment or jail choice has no positive
    /// bounty, [`Error::CannotPay`] when the player lacks enough gold, or
    /// [`Error::MissingGold`] when `Gold001` is unavailable.
    pub fn resolve_arrest(&mut self, choice: ArrestChoice) -> Result<ArrestResult, Error> {
        let options = self.arrest_options()?;
        let player = self.player();
        let faction_id = self
            .actor(player)
            .ok_or(Error::Actor(player))?
            .crime_faction
            .ok_or(Error::NoCrimeFaction(player))?;
        match choice {
            ArrestChoice::Pay => {
                if options.bounty == 0 {
                    return Err(Error::NoBounty);
                }
                let gold = self.form("Gold001").ok_or(Error::MissingGold)?;
                if !options.can_pay {
                    return Err(Error::CannotPay);
                }
                let removed = self.remove_item(player, gold, options.bounty, None);
                if removed != options.bounty {
                    return Err(Error::CannotPay);
                }
                let faction = self
                    .faction(faction_id)
                    .ok_or(Error::NotTracked(faction_id))?;
                faction.crime_gold_violent = 0;
                faction.crime_gold_nonviolent = 0;
                Ok(ArrestResult::Paid {
                    amount: options.bounty,
                })
            }
            ArrestChoice::Jail => {
                if !options.can_serve_time {
                    return Err(Error::NoBounty);
                }
                let days = self.serve_jail_time(faction_id)?;
                Ok(ArrestResult::Jailed { days })
            }
            ArrestChoice::Resist => Ok(ArrestResult::Resisted),
        }
    }

    /// Serve the player's bounty in `faction` as jail time.
    ///
    /// The sentence is one day per 100 gold, rounded up and capped at seven
    /// days. Serving clears the faction bounty, advances game time, records
    /// the days served, and clears partial skill use experience.
    ///
    /// # Errors
    /// Returns [`Error::NotTracked`] when the faction is absent or does not
    /// track crime, and [`Error::Actor`] when the player cannot be built.
    pub fn serve_jail_time(&mut self, faction: GlobalFormId) -> Result<u32, Error> {
        let player = self.player();
        self.actor(player).ok_or(Error::Actor(player))?;
        let bounty = {
            let state = self.faction(faction).ok_or(Error::NotTracked(faction))?;
            if !state.tracks_crime {
                return Err(Error::NotTracked(faction));
            }
            state
                .crime_gold_violent
                .saturating_add(state.crime_gold_nonviolent)
                .max(0)
        };
        if bounty == 0 {
            return Ok(0);
        }
        self.confiscate_jail_property(faction, player);
        let days = sentence_days(bounty);
        let state = self.faction(faction).ok_or(Error::NotTracked(faction))?;
        state.crime_gold_violent = 0;
        state.crime_gold_nonviolent = 0;
        let actor = self.actor(player).ok_or(Error::Actor(player))?;
        actor.days_in_jail = actor.days_in_jail.saturating_add(days);
        actor.skill_experience.clear();
        let scale = self.clock.timescale;
        if scale.is_finite() && scale > 0.0 {
            self.advance_clock(f64::from(days) * 86_400.0 / scale);
        }
        Ok(days)
    }

    /// Clear the player's cumulative served-day counter.
    pub fn clear_prison(&mut self) {
        if let Some(player) = self.player {
            if let Some(actor) = self.actors.get_mut(&player) {
                actor.days_in_jail = 0;
            }
            if let Some(faction) = self
                .actors
                .get(&player)
                .and_then(|actor| actor.crime_faction)
            {
                self.restore_jail_property(faction, player);
            }
        }
    }

    /// Move player belongings to the faction's authored jail containers.
    /// Owner-marked stacks go to the evidence chest, while all other items go
    /// to the player belongings chest.
    fn confiscate_jail_property(&mut self, faction: GlobalFormId, player: GlobalFormId) {
        let Some(belongings) = self.faction_container(faction, T(b"PLCN")) else {
            return;
        };
        if self.reference(belongings).is_none() {
            return;
        }
        let evidence = self
            .faction_container(faction, T(b"STOL"))
            .filter(|chest| self.reference(*chest).is_some());

        let Some(player_inventory) = self.reference(player).map(|state| state.inventory.clone())
        else {
            return;
        };
        let Some(belongings_inventory) = self
            .reference(belongings)
            .map(|state| state.inventory.clone())
        else {
            return;
        };
        let evidence_inventory = evidence.and_then(|chest| {
            self.reference(chest)
                .map(|state| (chest, state.inventory.clone()))
        });
        let mut player_inventory = player_inventory;
        let mut belongings_inventory = belongings_inventory;
        let mut evidence_inventory = evidence_inventory;
        let mut unconfiscated = Vec::new();
        let items = player_inventory
            .iter()
            .map(|(item, count)| (*item, count))
            .collect::<Vec<_>>();
        for (item, _) in items {
            let stolen = player_inventory.take_stacks_where(item, |stack| {
                stack
                    .extra(33)
                    .is_some_and(|extra| matches!(extra.fields.as_slice(), [Field::Form(Some(_))]))
            });
            if let Some((_, chest)) = evidence_inventory.as_mut() {
                if !stolen.stacks().is_empty() && chest.receive(item, stolen).is_err() {
                    return;
                }
            } else if !stolen.stacks().is_empty() {
                unconfiscated.push((item, stolen));
            }
            let count = player_inventory.count(item);
            if count > 0 {
                let bundle = player_inventory.take(item, count);
                if belongings_inventory.receive(item, bundle).is_err() {
                    return;
                }
            }
        }
        for (item, bundle) in unconfiscated {
            if player_inventory.receive(item, bundle).is_err() {
                return;
            }
        }

        // CONTEXT: All inventory clones and receives were prepared before the live state changes.
        if let Some(state) = self.reference(player) {
            state.inventory = player_inventory;
        }
        if let Some(state) = self.reference(belongings) {
            state.inventory = belongings_inventory;
        }
        if let Some((chest, inventory)) = evidence_inventory {
            if let Some(state) = self.reference(chest) {
                state.inventory = inventory;
            }
        }
    }

    /// Return contents of the belongings chest to the player. Evidence chest
    /// contents remain confiscated.
    fn restore_jail_property(&mut self, faction: GlobalFormId, player: GlobalFormId) {
        let Some(belongings) = self.faction_container(faction, T(b"PLCN")) else {
            return;
        };
        let Some(player_inventory) = self.reference(player).map(|state| state.inventory.clone())
        else {
            return;
        };
        let Some(chest_inventory) = self
            .reference(belongings)
            .map(|state| state.inventory.clone())
        else {
            return;
        };
        let mut player_inventory = player_inventory;
        let mut chest_inventory = chest_inventory;
        let items = chest_inventory
            .iter()
            .map(|(item, count)| (*item, count))
            .collect::<Vec<_>>();
        for (item, count) in items {
            let bundle = chest_inventory.take(item, count);
            if player_inventory.receive(item, bundle).is_err() {
                return;
            }
        }
        // CONTEXT: Both inventory clones accepted every transfer before commit.
        if let Some(state) = self.reference(player) {
            state.inventory = player_inventory;
        }
        if let Some(state) = self.reference(belongings) {
            state.inventory = chest_inventory;
        }
    }

    fn faction_container(
        &self,
        faction: GlobalFormId,
        field: soe_esm::Tag,
    ) -> Option<GlobalFormId> {
        let record = self.record(faction)?;
        let bytes = record.find(field)?;
        if bytes.len() != 4 {
            return None;
        }
        self.resolve_from(faction, super::le_u32(bytes)?)
    }

    /// Whether any actor has witnessed a crime committed by `actor`.
    pub fn is_actor_victim(&self, actor: GlobalFormId) -> bool {
        self.actors
            .values()
            .any(|witness| witness.witnessed_crimes.contains(&actor))
    }

    /// Records a crime `perpetrator` commits, seen by `witness`. Prices it
    /// from `perpetrator`'s own crime faction's `CRVA` table
    /// (`Actor.GetCrimeFaction`) and adds the result to that faction's
    /// bounty alone, the violent or nonviolent split `Faction.ModCrimeGold`
    /// already keeps. A faction sharing the perpetrator's `CRGR` crime
    /// group is never touched, `CRGR` is not parsed (see docs/game.md).
    /// Marks `witness` as knowing `perpetrator` did it, in
    /// `ActorState::witnessed_crimes`, which persists across save and load.
    ///
    /// Every check runs before anything is mutated: an invalid witness or
    /// perpetrator, a perpetrator with no crime faction, or a crime faction
    /// that does not track crime all leave the world unchanged.
    ///
    /// The bounty and infamy ledgers use saturating arithmetic and clamp at
    /// zero, so a legitimately priced crime on top of an existing bounty
    /// never overflows the running total, matching `CrimeType::bounty`'s own
    /// saturating float-to-int cast in `crate::crime`.
    ///
    /// # Errors
    /// Returns [`Error::Actor`] when `witness` or `perpetrator` is not a
    /// valid actor reference, [`Error::NoCrimeFaction`] when `perpetrator`
    /// has none set, and [`Error::NotTracked`] when its crime faction does
    /// not exist or does not track crime.
    pub fn record_witnessed_crime(
        &mut self,
        perpetrator: GlobalFormId,
        witness: GlobalFormId,
        crime: CrimeType,
    ) -> Result<i32, Error> {
        self.actor(witness).ok_or(Error::Actor(witness))?;
        let faction_id = self
            .actor(perpetrator)
            .ok_or(Error::Actor(perpetrator))?
            .crime_faction
            .ok_or(Error::NoCrimeFaction(perpetrator))?;
        self.record_witnessed_crime_in_faction(perpetrator, witness, crime, faction_id)
    }

    /// Price a witnessed crime for the faction the witness reports to.
    pub(super) fn record_witnessed_crime_in_faction(
        &mut self,
        perpetrator: GlobalFormId,
        witness: GlobalFormId,
        crime: CrimeType,
        faction_id: GlobalFormId,
    ) -> Result<i32, Error> {
        self.actor(witness).ok_or(Error::Actor(witness))?;
        self.actor(perpetrator).ok_or(Error::Actor(perpetrator))?;
        let values = self.crime_values(faction_id);
        let fs = self
            .faction(faction_id)
            .ok_or(Error::NotTracked(faction_id))?;
        if !fs.tracks_crime {
            return Err(Error::NotTracked(faction_id));
        }
        let amount = crime.bounty(&values);
        if crime.is_violent() {
            fs.crime_gold_violent = fs.crime_gold_violent.saturating_add(amount).max(0);
            fs.infamy_violent = fs.infamy_violent.saturating_add(amount.max(0));
        } else {
            fs.crime_gold_nonviolent = fs.crime_gold_nonviolent.saturating_add(amount).max(0);
            fs.infamy_nonviolent = fs.infamy_nonviolent.saturating_add(amount.max(0));
        }
        let total = fs
            .crime_gold_violent
            .saturating_add(fs.crime_gold_nonviolent);
        self.actor(witness)
            .ok_or(Error::Actor(witness))?
            .witnessed_crimes
            .insert(perpetrator);
        Ok(total)
    }
}

fn sentence_days(bounty: i32) -> u32 {
    u32::try_from(bounty.max(0).saturating_add(99) / 100)
        .unwrap_or(u32::MAX)
        .clamp(1, 7)
}

/// Save the player's jail-day count in the PRG-13 actor section.
pub(crate) fn write_days_in_jail(
    actor: &super::ActorState,
    writer: &mut Writer,
    _pool: &mut ArrayPool,
) {
    writer.u32(actor.days_in_jail);
}

/// Restore the player's jail-day count from the PRG-13 actor section.
pub(crate) fn read_days_in_jail(
    actor: &mut super::ActorState,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    actor.days_in_jail = reader.u32()?;
    Ok(())
}
