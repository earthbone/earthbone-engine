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
    /// Read a static base's template group and the owner of its raw links.
    /// A leveled link has no single result without a spawned reference, so
    /// base-form queries retain the editor base's fields in that case.
    ///
    /// # Errors
    /// Rejects missing, deleted or cyclic NPC records.
    pub(crate) fn npc_part(
        &self,
        base: GlobalFormId,
        group: u16,
    ) -> Result<(GlobalFormId, Npc), Error> {
        let mut current = base;
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(current) || visited.len() > 128 {
                return Err(Error::Invalid(
                    "cyclic or excessive NPC template chain".into(),
                ));
            }
            let npc = self.npc_record(current)?;
            let Some(raw) = npc
                .template
                .filter(|raw| *raw != 0 && npc.uses_template(group))
            else {
                return Ok((current, npc));
            };
            let next = self
                .resolve_from(current, raw)
                .ok_or_else(|| Error::Invalid("missing NPC template".into()))?;
            if self.tag_of(next) == Some(Tag(*b"LVLN")) {
                return Ok((base, self.npc_record(base)?));
            }
            current = next;
        }
    }

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
        self.npc_race(source)
    }

    /// Overall linear scale for an actor's runtime-generated controller
    /// capsule: `race.height[sex] * npc.height`, following
    /// `TESNPC::GetHeight()` (CommonLibSSE-NG `src/RE/T/TESNPC.cpp`). Sex
    /// and height both come from the NPC that supplies the Traits template
    /// group, the same source [`Self::actor_base_race`] reads: the Creation
    /// Kit groups race, sex, height and weight together under "Use Traits".
    /// An actor base with no race or no RACE `DATA` uses scale 1.0.
    ///
    /// # Errors
    /// Rejects invalid templates, an unresolved race link, or a non-finite
    /// or non-positive resulting scale.
    pub fn actor_controller_scale(&self, base: GlobalFormId) -> Result<f32, Error> {
        let traits = self.npc_component_source(base, npc::template::TRAITS)?;
        let npc = self.npc_record(traits)?;
        let race_height = self
            .npc_race(traits)?
            .and_then(|id| self.record(id))
            .map(Attributes::from_record)
            .transpose()?
            .flatten()
            .map_or(1.0, |race| {
                if npc.is_female() {
                    race.height_female
                } else {
                    race.height_male
                }
            });
        let scale = race_height * npc.height;
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Error::Invalid(
                "non-finite or non-positive actor controller scale".into(),
            ));
        }
        Ok(scale)
    }

    pub(super) fn npc_race(&self, source: GlobalFormId) -> Result<Option<GlobalFormId>, Error> {
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
        let traits = self.npc_component_source(base, npc::template::TRAITS)?;
        self.actor_values_from_sources(source, traits)
    }

    pub(super) fn actor_values_from_sources(
        &self,
        source: GlobalFormId,
        traits: GlobalFormId,
    ) -> Result<Vec<(String, f32)>, Error> {
        let npc = self.npc_record(source)?;
        let mut values = npc.base_values();
        let race = self.npc_race(traits)?;
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

    pub(crate) fn npc_record(&self, id: GlobalFormId) -> Result<Npc, Error> {
        let record = self
            .record(id)
            .ok_or_else(|| Error::Invalid(format!("missing NPC {id}")))?;
        if record.is_deleted() {
            return Err(Error::Invalid(format!("deleted NPC {id}")));
        }
        Npc::from_record(record, self.record_text_localized(id, Tag(*b"FULL")))
    }
}
