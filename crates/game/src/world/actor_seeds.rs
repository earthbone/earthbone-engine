// SPDX-License-Identifier: GPL-3.0-or-later

//! Attribute sources follow NPC ACBS template groups, resolving each TPLT
//! in its winning file. RACE starting values plus NPC offsets follow the
//! skymp GetBaseActorValues comparison and UESP NPC/RACE layouts. DNAM's
//! auto-calculated fields are cached inputs, not a live level-scaling formula.

use std::collections::BTreeSet;

use soe_esm::{Error, GlobalFormId, Tag};

use crate::{
    npc::{self, Npc},
    race::Attributes,
    World,
};

impl World {
    /// Find the NPC supplying one template group without crossing file namespaces.
    ///
    /// # Errors
    /// Missing/deleted records, template cycles and unresolved leveled templates.
    pub fn npc_component_source(
        &self,
        base: GlobalFormId,
        group: u16,
    ) -> Result<GlobalFormId, Error> {
        let mut current = base;
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(current) || visited.len() > 64 {
                return Err(Error::Invalid(
                    "cyclic or excessive NPC template chain".into(),
                ));
            }
            let npc = self.npc_record(current)?;
            let Some(raw) = npc
                .template
                .filter(|raw| *raw != 0 && npc.config.template_flags & group != 0)
            else {
                return Ok(current);
            };
            current = self
                .resolve_from(current, raw)
                .ok_or_else(|| Error::Invalid("missing NPC template".into()))?;
        }
    }

    /// Resolve a race from the NPC that supplies traits.
    ///
    /// # Errors
    /// Rejects malformed or unresolved template chains and race links.
    pub fn actor_base_race(&self, base: GlobalFormId) -> Result<Option<GlobalFormId>, Error> {
        let source = self.npc_component_source(base, npc::template::TRAITS)?;
        let npc = self.npc_record(source)?;
        let Some(raw) = npc.race.filter(|raw| *raw != 0) else {
            return Ok(None);
        };
        let race = self
            .resolve_from(source, raw)
            .ok_or_else(|| Error::Invalid("unresolved NPC race".into()))?;
        if self
            .record(race)
            .is_none_or(|record| record.is_deleted() || record.tag != Tag(*b"RACE"))
        {
            return Err(Error::Invalid("missing or wrong NPC race".into()));
        }
        Ok(Some(race))
    }

    /// Initial values for the shared actor store, from winning race and NPC inputs.
    /// Cached DNAM auto-calculation is retained until class/level calculation exists.
    ///
    /// # Errors
    /// Rejects invalid templates, race attributes and overflowing attribute sums.
    pub fn initial_actor_values(&self, base: GlobalFormId) -> Result<Vec<(String, f32)>, Error> {
        let source = self.npc_component_source(base, npc::template::STATS)?;
        let npc = self.npc_record(source)?;
        let mut values = npc.base_values();
        let race = self.actor_base_race(base)?;
        let attributes = race
            .and_then(|id| self.record(id))
            .map(Attributes::from_record)
            .transpose()?
            .flatten();
        if npc.config.flags & npc::flags::AUTO_CALC_STATS != 0 {
            if let Some(stats) = npc.stats {
                values[0].1 = f32::from(stats.health);
                values[1].1 = f32::from(stats.magicka);
                values[2].1 = f32::from(stats.stamina);
            }
        } else if let Some(race) = attributes {
            for (index, start, offset) in [
                (0, race.health, npc.config.health_offset),
                (1, race.magicka, npc.config.magicka_offset),
                (2, race.stamina, npc.config.stamina_offset),
            ] {
                let value = (f64::from(start) + f64::from(offset)) as f32;
                if !value.is_finite() {
                    return Err(Error::Invalid("overflowing NPC attribute".into()));
                }
                values[index].1 = value;
            }
        }
        values.push(("speedmult".into(), f32::from(npc.config.speed)));
        if let Some(race) = attributes {
            values.extend([
                ("carryweight".into(), race.carry_weight),
                ("mass".into(), race.mass),
                ("healrate".into(), race.heal_rate),
                ("magickarate".into(), race.magicka_rate),
                ("staminarate".into(), race.stamina_rate),
                ("unarmeddamage".into(), race.unarmed_damage),
            ]);
        }
        Ok(values)
    }

    fn npc_record(&self, id: GlobalFormId) -> Result<Npc, Error> {
        let reference = self
            .order()
            .winner_ref(id)
            .ok_or_else(|| Error::Invalid(format!("missing NPC {id}")))?;
        let record = self.order().record(reference);
        if record.is_deleted() {
            return Err(Error::Invalid(format!("deleted NPC {id}")));
        }
        Npc::from_record(
            record,
            self.order()
                .file(reference.file)
                .plugin
                .header
                .is_localized(),
        )
    }
}
