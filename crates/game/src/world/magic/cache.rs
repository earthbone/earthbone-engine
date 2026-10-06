// SPDX-License-Identifier: GPL-3.0-or-later

//! Decoded winning SPEL and MGEF views, following OpenMW's immutable spell
//! store. Runtime overrides bypass the cache. Actor values and condition
//! results stay live in World, rather than in these record-derived views.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use soe_esm::records::gameplay::{MagicEffect, Spell};
use soe_esm::GlobalFormId;

use super::{ActiveEffect, EffectId, MagicError, World, T};

#[derive(Clone, Copy)]
pub(super) struct TickDefinition {
    pub archetype: u32,
    pub no_death_dispel: bool,
    pub conditions: bool,
}

#[derive(Default)]
pub(crate) struct MagicDefinitions {
    effects: HashMap<GlobalFormId, Rc<MagicEffect>>,
    spells: HashMap<GlobalFormId, Rc<Spell>>,
    ticks: HashMap<(GlobalFormId, GlobalFormId, u32), TickDefinition>,
}

#[derive(Default)]
pub(crate) struct EffectIndex {
    by_archetype: BTreeMap<u32, BTreeSet<EffectId>>,
    summoned: BTreeSet<EffectId>,
}

impl World {
    pub(super) fn insert_active_effect(&mut self, id: EffectId, effect: ActiveEffect) {
        if !self.active_effects.contains_key(&id) {
            if let Ok(definition) = self.magic_effect_definition(effect.effect) {
                self.effect_index
                    .by_archetype
                    .entry(definition.data.archetype)
                    .or_default()
                    .insert(id);
            }
        }
        if effect.summoned.is_some() {
            self.effect_index.summoned.insert(id);
        } else {
            self.effect_index.summoned.remove(&id);
        }
        self.active_effects.insert(id, effect);
    }

    pub(super) fn unindex_active_effect(&mut self, id: EffectId) {
        for effects in self.effect_index.by_archetype.values_mut() {
            effects.remove(&id);
        }
        self.effect_index.summoned.remove(&id);
    }

    pub(crate) fn rebuild_effect_index(&mut self) {
        self.effect_index = EffectIndex::default();
        for effect in self.active_effects.values() {
            if let Ok(definition) = self.magic_effect_definition(effect.effect) {
                self.effect_index
                    .by_archetype
                    .entry(definition.data.archetype)
                    .or_default()
                    .insert(effect.id);
            }
            if effect.summoned.is_some() {
                self.effect_index.summoned.insert(effect.id);
            }
        }
    }

    pub(super) fn effects_of_archetype(
        &self,
        archetype: u32,
    ) -> impl Iterator<Item = &ActiveEffect> {
        self.effect_index
            .by_archetype
            .get(&archetype)
            .into_iter()
            .flatten()
            .filter_map(|id| self.active_effects.get(id))
    }

    /// References owned by active effects, in stable effect identity order.
    pub fn summoned_effect_references(&self) -> impl Iterator<Item = GlobalFormId> + '_ {
        self.effect_index.summoned.iter().filter_map(|id| {
            self.active_effects
                .get(id)
                .and_then(|effect| effect.summoned)
        })
    }

    /// Active Grab Actor applications without visiting unrelated actor state.
    pub fn grab_actor_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.effects_of_archetype(45)
    }

    pub(super) fn effect_tick_definition(
        &self,
        effect: &ActiveEffect,
    ) -> Result<TickDefinition, MagicError> {
        let runtime = self.record_overrides.contains_key(&effect.effect)
            || self.created_base_records.contains_key(&effect.effect);
        let key = (effect.spell, effect.effect, effect.effect_index);
        if !runtime {
            if let Some(definition) = self.magic_definitions.borrow().ticks.get(&key) {
                return Ok(*definition);
            }
        }
        let base = self.magic_effect_definition(effect.effect)?;
        let conditions = if self.tag_of(effect.spell) == Some(T(b"SPEL")) {
            let spell = self.spell_definition(effect.spell)?;
            let entry = spell
                .effects
                .get(effect.effect_index as usize)
                .ok_or(MagicError::Unsupported("active effect entry"))?;
            !entry.conditions.is_empty() || !base.conditions.is_empty()
        } else {
            false
        };
        let definition = TickDefinition {
            archetype: base.data.archetype,
            no_death_dispel: base.data.flags & super::NO_DEATH_DISPEL != 0,
            conditions,
        };
        if !runtime {
            self.magic_definitions
                .borrow_mut()
                .ticks
                .insert(key, definition);
        }
        Ok(definition)
    }

    pub(super) fn magic_effect_definition(
        &self,
        id: GlobalFormId,
    ) -> Result<Rc<MagicEffect>, MagicError> {
        let runtime =
            self.record_overrides.contains_key(&id) || self.created_base_records.contains_key(&id);
        if !runtime {
            if let Some(definition) = self.magic_definitions.borrow().effects.get(&id) {
                return Ok(Rc::clone(definition));
            }
        }
        let record = self.record(id).ok_or(MagicError::Missing(id))?;
        if record.is_deleted() {
            return Err(MagicError::Missing(id));
        }
        let definition = Rc::new(MagicEffect::from_record(
            record,
            self.record_text_localized(id, T(b"FULL")),
        )?);
        if !runtime {
            self.magic_definitions
                .borrow_mut()
                .effects
                .insert(id, Rc::clone(&definition));
        }
        Ok(definition)
    }

    pub(super) fn spell_definition(&self, id: GlobalFormId) -> Result<Rc<Spell>, MagicError> {
        if let Some(definition) = self.magic_definitions.borrow().spells.get(&id) {
            return Ok(Rc::clone(definition));
        }
        let rref = self.order.winner_ref(id).ok_or(MagicError::Missing(id))?;
        let record = self.order.record(rref);
        if record.is_deleted() {
            return Err(MagicError::Missing(id));
        }
        let definition = Rc::new(Spell::from_record(
            record,
            self.order.file(rref.file).plugin.header.is_localized(),
        )?);
        self.magic_definitions
            .borrow_mut()
            .spells
            .insert(id, Rc::clone(&definition));
        Ok(definition)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::record_overrides::RecordOverride;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record};

    fn fixture() -> (World, GlobalFormId) {
        let id = GlobalFormId {
            file: 0,
            object: 0x800,
        };
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(T(b"MGEF"), id.object).sub(T(b"DATA"), vec![0; 152]));
        let mut order = LoadOrder::new();
        order.add("Magic.esm", plugin).unwrap();
        (World::new(order), id)
    }

    #[test]
    fn prf_35_immutable_definitions_decode_once_and_share_their_view() {
        let (world, id) = fixture();
        let first = world.magic_effect_definition(id).unwrap();
        let second = world.magic_effect_definition(id).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn prf_35_runtime_override_bypasses_warmed_definition_and_deleted_form_rejects() {
        let (mut world, id) = fixture();
        let first = world.magic_effect_definition(id).unwrap();
        let mut data = vec![0; 152];
        data[64..68].copy_from_slice(&12_u32.to_le_bytes());
        let record = Record::new(T(b"MGEF"), id.object).sub(T(b"DATA"), data);
        world
            .record_overrides
            .insert(id, RecordOverride::new(id, record));
        assert_eq!(
            world.magic_effect_definition(id).unwrap().data.archetype,
            12
        );
        assert_eq!(first.data.archetype, 0);
        world.record_overrides.get_mut(&id).unwrap().record.flags |= 0x20;
        assert!(matches!(
            world.magic_effect_definition(id),
            Err(MagicError::Missing(_))
        ));
    }

    #[test]
    fn prf_35_malformed_definitions_never_enter_the_cache() {
        let (mut world, id) = fixture();
        let malformed = Record::new(T(b"MGEF"), id.object).sub(T(b"DATA"), vec![0; 3]);
        world
            .record_overrides
            .insert(id, RecordOverride::new(id, malformed));
        assert!(world.magic_effect_definition(id).is_err());
        let missing = GlobalFormId {
            object: id.object + 1,
            ..id
        };
        assert!(world.magic_effect_definition(missing).is_err());
        assert!(world.magic_definitions.borrow().effects.is_empty());
    }
}
