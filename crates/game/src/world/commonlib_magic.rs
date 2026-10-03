// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib magic contracts backed by the live `World` records.

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{CannotCastReason, MagicWorldState, UnsupportedState};
use soe_esm::{GlobalFormId, Tag};

use crate::world::{Alloc, World};

const MARKER_VISIBLE: u8 = 1;
const MARKER_CAN_TRAVEL: u8 = 2;

/// A short-lived adapter over actor, magic, reference and inventory state.
pub struct CommonLibMagic<'world, 'alloc> {
    world: &'world mut World,
    alloc: Alloc<'alloc>,
}

impl<'world, 'alloc> CommonLibMagic<'world, 'alloc> {
    /// Bind the MIT magic provider to one live world and object allocator.
    pub fn new(world: &'world mut World, alloc: Alloc<'alloc>) -> Self {
        Self { world, alloc }
    }

    fn resolve(&self, id: FormId) -> Result<GlobalFormId, UnsupportedState> {
        self.world
            .from_runtime_id(id.0)
            .ok_or(UnsupportedState::MissingReference)
    }

    fn actor(&mut self, id: FormId) -> Result<GlobalFormId, UnsupportedState> {
        let id = self.resolve(id)?;
        if self.world.actor(id).is_none() {
            return Err(UnsupportedState::MissingActor);
        }
        Ok(id)
    }
}

impl MagicWorldState for CommonLibMagic<'_, '_> {
    fn is_favorite(&self, actor: FormId, form: FormId) -> Result<bool, UnsupportedState> {
        let actor = self.resolve(actor)?;
        let form = self.resolve(form)?;
        self.world
            .actor_ref(actor)
            .map(|state| state.favorites.contains(&form))
            .ok_or(UnsupportedState::MissingActor)
    }

    fn set_favorite(
        &mut self,
        actor: FormId,
        form: FormId,
        favorite: bool,
    ) -> Result<bool, UnsupportedState> {
        let actor = self.actor(actor)?;
        let form = self.resolve(form)?;
        let state = self
            .world
            .actor(actor)
            .ok_or(UnsupportedState::MissingActor)?;
        Ok(if favorite {
            state.favorites.insert(form)
        } else {
            state.favorites.remove(&form)
        })
    }

    fn cannot_cast(
        &mut self,
        actor: FormId,
        spell: FormId,
    ) -> Result<CannotCastReason, UnsupportedState> {
        let actor = self.actor(actor)?;
        let spell = self.resolve(spell)?;
        if self.world.tag_of(spell) != Some(Tag::new(b"SPEL")) {
            return Err(UnsupportedState::Hook(
                "MagicSystem::CannotCast invalid spell",
            ));
        }
        if self.world.pending_spell_cast(actor).is_some() {
            return Ok(CannotCastReason::MultipleCast);
        }
        let definition = self
            .world
            .spell(spell)
            .map_err(|_| UnsupportedState::Hook("MagicSystem::CannotCast unsupported spell"))?;
        if matches!(definition.data.spell_type, 2 | 3)
            && self.world.voice_recovery_time(actor) > 0.0
        {
            return Ok(CannotCastReason::PowerUsed);
        }
        let cost = self.world.spell_cost(spell, actor).map_err(|_| {
            UnsupportedState::Hook("MagicSystem::CannotCast unsupported spell cost")
        })?;
        let magicka = self
            .world
            .actor_ref(actor)
            .map(|state| state.values.get("magicka").current())
            .ok_or(UnsupportedState::MissingActor)?;
        Ok(if magicka < cost {
            CannotCastReason::Magicka
        } else {
            CannotCastReason::Ok
        })
    }

    fn has_magic_effect(&self, target: FormId, effect: FormId) -> Result<bool, UnsupportedState> {
        let target = self.resolve(target)?;
        let effect = self.resolve(effect)?;
        if self.world.reference_ref(target).is_none() {
            return Err(UnsupportedState::MissingReference);
        }
        Ok(self
            .world
            .active_effects()
            .any(|active| active.target == target && active.effect == effect))
    }

    fn active_effect_count(&self, target: FormId) -> Result<usize, UnsupportedState> {
        let target = self.resolve(target)?;
        if self.world.reference_ref(target).is_none() {
            return Err(UnsupportedState::MissingReference);
        }
        Ok(self
            .world
            .active_effects()
            .filter(|active| active.target == target)
            .count())
    }

    fn dispel_spell(&mut self, target: FormId, spell: FormId) -> Result<bool, UnsupportedState> {
        let target = self.resolve(target)?;
        let spell = self.resolve(spell)?;
        if self.world.reference_ref(target).is_none() {
            return Err(UnsupportedState::MissingReference);
        }
        Ok(self.world.dispel_spell(target, spell, self.alloc))
    }

    fn map_marker_flags(&self, reference: FormId) -> Result<u8, UnsupportedState> {
        let reference = self.resolve(reference)?;
        let state = self
            .world
            .reference_ref(reference)
            .ok_or(UnsupportedState::MissingReference)?;
        Ok((u8::from(state.on_map) * MARKER_VISIBLE)
            | (u8::from(state.fast_travel) * MARKER_CAN_TRAVEL))
    }

    fn set_map_marker_flags(
        &mut self,
        reference: FormId,
        flags: u8,
    ) -> Result<(), UnsupportedState> {
        if flags & !(MARKER_VISIBLE | MARKER_CAN_TRAVEL) != 0 {
            return Err(UnsupportedState::Hook(
                "MapMarkerData::ShowAllHidden has no runtime state",
            ));
        }
        let reference = self.resolve(reference)?;
        let state = self
            .world
            .reference(reference)
            .ok_or(UnsupportedState::MissingReference)?;
        state.on_map = flags & MARKER_VISIBLE != 0;
        state.fast_travel = flags & MARKER_CAN_TRAVEL != 0;
        Ok(())
    }

    fn equip_object(
        &mut self,
        actor: FormId,
        object: FormId,
        params: soe_commonlib_shim::world::ObjectEquipParams,
    ) -> Result<(), UnsupportedState> {
        if !params.is_supported() {
            return Err(UnsupportedState::Hook(
                "ActorEquipManager::EquipObject unsupported ObjectEquipParams",
            ));
        }
        let actor = self.actor(actor)?;
        let object = self.resolve(object)?;
        self.world
            .equip_item(actor, object)
            .map_err(|_| UnsupportedState::Hook("ActorEquipManager::EquipObject rejected item"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{ActorState, RefState};
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn fixture_world() -> World {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"NPC_"), 1));
        plugin.push_top(Record::new(Tag(*b"SPEL"), 2).sub(Tag(*b"SPIT"), vec![0; 36]));
        plugin.push_top(Record::new(Tag(*b"MGEF"), 3));
        let mut order = LoadOrder::new();
        order.add("magic_fixture.esp", plugin).unwrap();
        World::new(order)
    }

    fn allocate(_name: &str, _class: &str) -> soe_papyrus::ObjectId {
        soe_papyrus::ObjectId(1)
    }

    #[test]
    fn shm_13_favorites_and_magic_target_use_shared_actor_and_effect_state() {
        let actor = id(1);
        let spell = id(2);
        let effect = id(3);
        let mut world = fixture_world();
        world.actors.insert(actor, ActorState::default());
        world.refs.insert(
            actor,
            RefState {
                is_actor: true,
                ..RefState::new(None)
            },
        );
        world.active_effects.insert(
            crate::world::magic::EffectId(1),
            crate::world::magic::ActiveEffect {
                id: crate::world::magic::EffectId(1),
                spell,
                effect,
                effect_index: 0,
                source: spell,
                caster: Some(actor),
                target: actor,
                magnitude: 1.0,
                duration: Some(2.0),
                taper_weight: 0.0,
                taper_curve: 0.0,
                taper_duration: 0.0,
                elapsed: 0.0,
                summoned: None,
                granted_perk: None,
                reanimated_previous_owner: None,
                restore_owner: None,
                recover: false,
                changes: Vec::new(),
            },
        );
        let actor_form = FormId(world.order().runtime_id(actor));
        let spell_form = FormId(world.order().runtime_id(spell));
        let effect_form = FormId(world.order().runtime_id(effect));
        {
            let mut allocator = allocate;
            let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);
            assert!(!adapter.is_favorite(actor_form, spell_form).unwrap());
            assert!(adapter.set_favorite(actor_form, spell_form, true).unwrap());
            assert!(adapter.is_favorite(actor_form, spell_form).unwrap());
            assert!(adapter.has_magic_effect(actor_form, effect_form).unwrap());
            assert_eq!(adapter.active_effect_count(actor_form).unwrap(), 1);
            assert!(!adapter.has_magic_effect(actor_form, spell_form).unwrap());
            assert!(adapter
                .is_favorite(actor_form, FormId(0xdead_beef))
                .is_err());
            assert_eq!(
                adapter.cannot_cast(actor_form, spell_form).unwrap(),
                CannotCastReason::Ok
            );
            adapter.world.spell_casts_in_progress.insert(
                actor,
                crate::world::magic::SpellCast {
                    spell,
                    target: None,
                    kind: crate::world::magic::SpellCastKind::FireAndForget,
                },
            );
            assert_eq!(
                adapter.cannot_cast(actor_form, spell_form).unwrap(),
                CannotCastReason::MultipleCast
            );
        }
        world.active_effects.clear();
        let bytes = world.save_state();
        let mut restored = fixture_world();
        restored.load_state(&bytes).unwrap();
        assert!(restored.actors[&actor].favorites.contains(&spell));
    }

    #[test]
    fn shm_13_marker_and_equip_adapters_preserve_supported_fields_and_reject_hooks() {
        let reference = id(4);
        let actor = id(4);
        let mut world = fixture_world();
        world.actors.insert(actor, ActorState::default());
        world.refs.insert(
            reference,
            RefState {
                is_actor: true,
                on_map: true,
                fast_travel: false,
                ..RefState::new(None)
            },
        );
        let reference_form = FormId(world.order().runtime_id(reference));
        let actor_form = reference_form;
        let item_form = FormId(world.order().runtime_id(id(3)));
        let mut allocator = allocate;
        let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);
        assert_eq!(
            adapter.map_marker_flags(reference_form).unwrap(),
            MARKER_VISIBLE
        );
        adapter
            .set_map_marker_flags(reference_form, MARKER_VISIBLE | MARKER_CAN_TRAVEL)
            .unwrap();
        assert_eq!(adapter.map_marker_flags(reference_form).unwrap(), 3);
        assert!(adapter.set_map_marker_flags(reference_form, 4).is_err());
        assert!(adapter
            .equip_object(
                actor_form,
                item_form,
                soe_commonlib_shim::world::ObjectEquipParams::default(),
            )
            .is_ok());
        let unsupported = soe_commonlib_shim::world::ObjectEquipParams {
            force_equip: true,
            ..Default::default()
        };
        assert!(adapter
            .equip_object(actor_form, FormId(0xdead_beef), unsupported)
            .is_err());
        assert!(adapter
            .cannot_cast(actor_form, FormId(0xdead_beef))
            .is_err());
        assert!(adapter
            .world
            .actor_ref(actor)
            .unwrap()
            .equipped
            .contains(&id(3)));
    }
}
