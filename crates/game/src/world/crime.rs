// SPDX-License-Identifier: GPL-3.0-or-later

//! Recording a witnessed crime. See `crate::crime` for the crime types, the
//! `FACT.CRVA` pricing table and this module's `Error`.

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;

use crate::crime::{CrimeType, Error};

use super::World;

impl World {
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
        let state = self.faction(faction).ok_or(Error::NotTracked(faction))?;
        if !state.tracks_crime {
            return Err(Error::NotTracked(faction));
        }
        let bounty = state
            .crime_gold_violent
            .saturating_add(state.crime_gold_nonviolent)
            .max(0);
        if bounty == 0 {
            return Ok(0);
        }
        let days = sentence_days(bounty);
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
        }
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
pub(crate) fn write_days_in_jail(actor: &super::ActorState, writer: &mut Writer) {
    writer.u32(actor.days_in_jail);
}

/// Restore the player's jail-day count from the PRG-13 actor section.
pub(crate) fn read_days_in_jail(
    actor: &mut super::ActorState,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), VmError> {
    actor.days_in_jail = reader.u32()?;
    Ok(())
}
