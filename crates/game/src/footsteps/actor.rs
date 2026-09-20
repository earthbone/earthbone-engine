// SPDX-License-Identifier: GPL-3.0-or-later

//! Actor footstep sets from NPC_/RACE skins and equipped ARMO/ARMA records.
//! Layouts follow xEdit. Armor-parent matching follows CommonLibSSE-NG's
//! TESObjectARMA::IsValidRace. Slot priority follows the authored DNAM values.
//! Equal-priority ordering and deduplicated layering are engine policy pending
//! a comparison with the original runtime.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::cursor::Fields;
use soe_esm::{Error, GlobalFormId, Record, Result};

use super::{Footsteps, T};
use crate::World;

#[derive(Default)]
struct Traits {
    race: Option<GlobalFormId>,
    skin: Option<GlobalFormId>,
    female: bool,
}

#[derive(Clone, Copy)]
struct Part {
    priority: u8,
    set: Option<GlobalFormId>,
}

impl World {
    /// Resolve from current actor state on each event, including equipment and
    /// SetRace changes. No independent audio copy of equipment is maintained.
    pub fn footstep_sets(&mut self, actor: GlobalFormId) -> Result<Vec<GlobalFormId>> {
        let base = if self.reference(actor).and_then(|r| r.base).is_some() {
            Some(self.actor_component_source(actor, crate::npc::template::TRAITS)?)
        } else {
            None
        };
        let state = self
            .actor(actor)
            .ok_or_else(|| Error::Invalid(format!("footsteps require an actor: {actor}")))?;
        let race = state.race;
        let equipped = state.equipped.clone();
        Footsteps::new(self.order()).actor_sets(base, race, &equipped)
    }
}

impl Footsteps<'_> {
    /// Select sets from the active armor addons. Skin is considered first,
    /// equipped armor second. Higher priority wins each occupied body slot.
    /// Ties use the later addon in record order, then sorted equipment order.
    /// Distinct surviving sets are emitted once, in FormID order.
    pub fn actor_sets(
        &self,
        base: Option<GlobalFormId>,
        current_race: Option<GlobalFormId>,
        equipped: &BTreeSet<GlobalFormId>,
    ) -> Result<Vec<GlobalFormId>> {
        let traits = base
            .map(|id| self.traits(id))
            .transpose()?
            .unwrap_or_default();
        let race = current_race.or(traits.race);
        let mut parts = BTreeMap::new();
        if let Some(race) = race {
            let races = self.armor_races(race)?;
            let (record, file) = self.record(race, T(b"RACE"))?;
            let skin = match traits.skin {
                Some(skin) => Some(skin),
                None => self.record_link(record, file, T(b"WNAM"))?,
            };
            if let Some(skin) = skin {
                self.armor_parts(skin, &races, traits.female, &mut parts)?;
            }
            for armor in equipped {
                // Weapons and other equipped forms do not carry armor addons.
                if self
                    .order
                    .winner(*armor)
                    .is_some_and(|r| r.tag == T(b"ARMO"))
                {
                    self.armor_parts(*armor, &races, traits.female, &mut parts)?;
                }
            }
        }
        let mut sets: BTreeSet<_> = parts.values().filter_map(|part| part.set).collect();
        if sets.is_empty() {
            if let Some(default) = self.default_object(*b"DFTS")? {
                self.record(default, T(b"FSTS"))?;
                sets.insert(default);
            }
        }
        Ok(sets.into_iter().collect())
    }

    fn traits(&self, mut base: GlobalFormId) -> Result<Traits> {
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(base) {
                return Err(Error::Invalid(format!(
                    "NPC_ trait template cycle at {base}"
                )));
            }
            let (record, file) = self.record(base, T(b"NPC_"))?;
            let (female, inherited) = if let Some(bytes) = record.find(T(b"ACBS")) {
                Fields::new(record.tag, T(b"ACBS"), bytes).exact(24)?;
                let flags = Fields::new(record.tag, T(b"ACBS"), bytes).u32()?;
                let template = u16::from_le_bytes([bytes[18], bytes[19]]);
                (
                    flags & crate::npc::flags::FEMALE != 0,
                    template & crate::npc::template::TRAITS != 0,
                )
            } else {
                (false, false)
            };
            if inherited {
                if let Some(template) = self.record_link(record, file, T(b"TPLT"))? {
                    base = template;
                    continue;
                }
            }
            return Ok(Traits {
                race: self.record_link(record, file, T(b"RNAM"))?,
                skin: self.record_link(record, file, T(b"WNAM"))?,
                female,
            });
        }
    }

    fn armor_races(&self, mut race: GlobalFormId) -> Result<BTreeSet<GlobalFormId>> {
        let mut races = BTreeSet::new();
        loop {
            if !races.insert(race) {
                return Err(Error::Invalid(format!("RACE armor-parent cycle at {race}")));
            }
            let (record, file) = self.record(race, T(b"RACE"))?;
            let Some(parent) = self.record_link(record, file, T(b"RNAM"))? else {
                return Ok(races);
            };
            race = parent;
        }
    }

    fn armor_parts(
        &self,
        armor: GlobalFormId,
        races: &BTreeSet<GlobalFormId>,
        female: bool,
        parts: &mut BTreeMap<u32, Part>,
    ) -> Result<()> {
        let (record, file) = self.record(armor, T(b"ARMO"))?;
        for bytes in record.all(T(b"MODL")) {
            let mut field = Fields::new(record.tag, T(b"MODL"), bytes);
            field.exact(4)?;
            let Some(addon) = self.link(file, field.u32()?) else {
                continue;
            };
            let (record, owner) = self.record(addon, T(b"ARMA"))?;
            let mut matches = self
                .record_link(record, owner, T(b"RNAM"))?
                .is_some_and(|race| races.contains(&race));
            for bytes in record.all(T(b"MODL")) {
                let mut field = Fields::new(record.tag, T(b"MODL"), bytes);
                field.exact(4)?;
                matches |= self
                    .link(owner, field.u32()?)
                    .is_some_and(|r| races.contains(&r));
            }
            if !matches {
                continue;
            }
            let data = record.required(T(b"DNAM"))?;
            Fields::new(record.tag, T(b"DNAM"), data).exact(12)?;
            let priority = data[usize::from(female)];
            let set = self.record_link(record, owner, T(b"SNDD"))?;
            if let Some(set) = set {
                self.record(set, T(b"FSTS"))?;
            }
            let slots = slots(record)?;
            for slot in 0..32 {
                if slots & (1 << slot) != 0
                    && parts
                        .get(&slot)
                        .is_none_or(|part| part.priority <= priority)
                {
                    // Silent addons also replace covered skin parts.
                    parts.insert(slot, Part { priority, set });
                }
            }
        }
        Ok(())
    }
}

fn slots(record: &Record) -> Result<u32> {
    for (tag, sizes) in [(T(b"BOD2"), &[4, 8][..]), (T(b"BODT"), &[8, 12][..])] {
        if let Some(data) = record.find(tag) {
            if !sizes.contains(&data.len()) {
                return Err(Error::Invalid(format!(
                    "{} {tag} has invalid length",
                    record.tag
                )));
            }
            return Fields::new(record.tag, tag, data).u32();
        }
    }
    Ok(0)
}
