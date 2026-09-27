// SPDX-License-Identifier: GPL-3.0-or-later

//! Attribute sources follow NPC ACBS template groups, resolving each TPLT
//! in its winning file. RACE starting values plus NPC offsets follow the
//! skymp GetBaseActorValues comparison and UESP NPC/RACE layouts. Health,
//! magicka and stamina, and skills, both follow UESP CLAS.wikitext's
//! "Weight Calculations": the class weighted formula for a fixed-level
//! auto-calculated actor with a resolvable class, RACE starts plus the
//! ACBS offset (attributes) for a manual actor, and DNAM's own skill cache
//! because manual-mode race-bonus semantics are unresolved. Other cases use
//! DNAM's cache (no class, a class with a zero
//! weight sum, or `PC_LEVEL_MULT`, whose player-scaled level this store
//! does not track).

use std::collections::BTreeSet;

use soe_esm::{Error, GlobalFormId, Tag};

use crate::{
    class::Class,
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

    /// Resolve the class an NPC's `CNAM` names, or `None` for no class, an
    /// unresolved link, or a link that does not land on a `CLAS` record.
    /// Unlike [`Self::npc_race`], a bad class link is not an error: the
    /// skill formula that reads it already falls back to DNAM's cache, and a
    /// stale or wrong `CNAM` is a real condition in shipped plugins, not a
    /// malformed store.
    ///
    /// # Errors
    /// Rejects a missing or deleted NPC at `source`.
    pub(super) fn npc_class(&self, source: GlobalFormId) -> Result<Option<GlobalFormId>, Error> {
        let npc = self.npc_record(source)?;
        let Some(raw) = npc.class.filter(|raw| *raw != 0) else {
            return Ok(None);
        };
        let Some(class) = self.resolve_from(source, raw) else {
            return Ok(None);
        };
        if self
            .record(class)
            .is_none_or(|record| record.is_deleted() || record.tag != Tag(*b"CLAS"))
        {
            return Ok(None);
        }
        Ok(Some(class))
    }

    /// Initial values for the shared actor store, from winning race and NPC inputs.
    /// Health, magicka, stamina and skills compute from the winning class and
    /// race where the formula applies, see the module documentation.
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
        // A class that fails to resolve or to parse is not a store error:
        // the skill formula below already falls back to DNAM's cache, see
        // `npc_class`.
        let class = self
            .npc_class(source)?
            .and_then(|id| self.record(id))
            .and_then(|record| Class::from_record(record).ok().flatten());
        if npc.config.flags & npc::flags::AUTO_CALC_STATS != 0 {
            if let Some(stats) = npc.stats {
                values[0].1 = f32::from(stats.health);
                values[1].1 = f32::from(stats.magicka);
                values[2].1 = f32::from(stats.stamina);
            }
            if npc.config.flags & npc::flags::PC_LEVEL_MULT == 0 {
                if let Some(class) = class {
                    let level = npc.level_for(0);
                    if let Some(attribute_values) =
                        class_attribute_values(level, attributes, &class)
                    {
                        for (index, value, offset) in [
                            (0, attribute_values[0], npc.config.health_offset),
                            (1, attribute_values[1], npc.config.magicka_offset),
                            (2, attribute_values[2], npc.config.stamina_offset),
                        ] {
                            let value = (f64::from(value) + f64::from(offset)) as f32;
                            if !value.is_finite() {
                                return Err(Error::Invalid("overflowing NPC attribute".into()));
                            }
                            values[index].1 = value;
                        }
                    }
                    if let Some(skills) = class_skill_values(level, attributes, &class) {
                        for (i, value) in skills.into_iter().enumerate() {
                            values[3 + i].1 = value;
                        }
                    }
                }
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

/// Local skill index (0 to 17, [`npc::SKILLS`] order) for a RACE `Skill
/// Boosts` full actor value index. `None` for an unused slot's zero pair or
/// any actor value outside the skill range (6 to 23).
fn race_skill_index(full_actor_value: i8) -> Option<usize> {
    let local = full_actor_value.checked_sub(6)?;
    usize::try_from(local).ok().filter(|index| *index < 18)
}

/// The race's boost for one local skill index, 0 when the race carries none.
fn race_skill_boost(race: &Attributes, local: usize) -> i8 {
    race.skill_boosts
        .iter()
        .find_map(|&(full, boost)| (race_skill_index(full) == Some(local)).then_some(boost))
        .unwrap_or(0)
}

/// Skill values from UESP CLAS.wikitext's "Weight Calculations" formula for
/// a fixed-level auto-calculated actor: `15 + racial bonus + complete sets +
/// leftover points`. Complete sets come from integer division of the level's
/// point pool by the class' summed weights. Leftover points go one at a time
/// to skills ordered by weight descending then actor value index ascending,
/// each capped at its own weight, repeating until none remain. `None` when
/// the class has no weighted skill to distribute to.
fn class_skill_values(level: u32, race: Option<Attributes>, class: &Class) -> Option<[f32; 18]> {
    let total_weight: u32 = class
        .skill_weights
        .iter()
        .map(|&weight| u32::from(weight))
        .sum();
    if total_weight == 0 {
        return None;
    }
    let points = 8 * level.saturating_sub(1);
    let per_set = points / total_weight;
    let mut remaining = points % total_weight;
    let mut extra = [0u32; 18];
    let mut order: Vec<usize> = (0..18).filter(|&i| class.skill_weights[i] > 0).collect();
    order.sort_by(|&a, &b| {
        class.skill_weights[b]
            .cmp(&class.skill_weights[a])
            .then(a.cmp(&b))
    });
    while remaining > 0 {
        let mut moved = false;
        for &i in &order {
            if remaining == 0 {
                break;
            }
            if extra[i] < u32::from(class.skill_weights[i]) {
                extra[i] += 1;
                remaining -= 1;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    let mut skills = [0.0f32; 18];
    for (i, skill) in skills.iter_mut().enumerate() {
        let racial = race.map_or(0, |race| i32::from(race_skill_boost(&race, i)));
        let weight = u32::from(class.skill_weights[i]);
        let raw = 15 + racial + (per_set * weight) as i32 + extra[i] as i32;
        *skill = raw.clamp(0, 100) as f32;
    }
    Some(skills)
}

/// Health, magicka and stamina from UESP CLAS.wikitext's "Weight
/// Calculations" formula for a fixed-level auto-calculated actor: `racial
/// start + 10*(Level-1) distributed by attribute weight`, with an extra
/// `5*(Level-1)` folded into health regardless of the class weights. Complete
/// sets come from integer division of the level's point pool by the class'
/// summed attribute weights. Leftover points go one at a time to attributes
/// ordered by weight descending then actor value index descending (stamina,
/// magicka, then health), each capped at its own weight, repeating until
/// none remain. Indices follow [`Class::attribute_weights`]: 0 health, 1
/// magicka, 2 stamina. `None` when the class has no weighted attribute to
/// distribute to, so the caller keeps the DNAM cache. The caller adds the
/// NPC's own ACBS offset, the same as the manual (non auto-calculated)
/// branch, since this formula only covers the class-weighted racial start.
fn class_attribute_values(level: u32, race: Option<Attributes>, class: &Class) -> Option<[f32; 3]> {
    let total_weight: u32 = class
        .attribute_weights
        .iter()
        .map(|&weight| u32::from(weight))
        .sum();
    if total_weight == 0 {
        return None;
    }
    let level_steps = level.saturating_sub(1);
    let points = 10 * level_steps;
    let per_set = points / total_weight;
    let mut remaining = points % total_weight;
    let mut extra = [0u32; 3];
    let mut order: Vec<usize> = (0..3).filter(|&i| class.attribute_weights[i] > 0).collect();
    order.sort_by(|&a, &b| {
        class.attribute_weights[b]
            .cmp(&class.attribute_weights[a])
            .then(b.cmp(&a))
    });
    while remaining > 0 {
        let mut moved = false;
        for &i in &order {
            if remaining == 0 {
                break;
            }
            if extra[i] < u32::from(class.attribute_weights[i]) {
                extra[i] += 1;
                remaining -= 1;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    let racial = race.map_or([0.0, 0.0, 0.0], |race| {
        [race.health, race.magicka, race.stamina]
    });
    let mut values = [0.0f32; 3];
    for (i, value) in values.iter_mut().enumerate() {
        let weight = u32::from(class.attribute_weights[i]);
        let weighted = (per_set * weight + extra[i]) as f32;
        let health_bonus = if i == 0 {
            5.0 * level_steps as f32
        } else {
            0.0
        };
        *value = racial[i] + health_bonus + weighted;
    }
    Some(values)
}
