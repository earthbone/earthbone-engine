// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only CommonLib-shaped views over state already owned by `World`.

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::ni::NiPoint3;
use soe_commonlib_shim::world::{
    ReanimateEffect, RuntimeState, Setting, SettingValue as RuntimeSettingValue, SitSleepState,
    Sky, UnsupportedWorldHook,
};
use soe_commonlib_shim::world_types::{
    self, HazardState, HighProcessState, HitData, SettingValue, UnsupportedHook, WornState,
};
use soe_esm::GlobalFormId;
use soe_papyrus::Value;

use super::World;
use crate::combat::{HitResult, MeleeHit};

impl World {
    fn shim_id(&self, id: GlobalFormId) -> FormId {
        FormId(self.order.runtime_id(id))
    }

    /// Read the selected item's ExtraWorn markers from the actor inventory.
    #[must_use]
    pub fn commonlib_extra_worn(
        &self,
        actor: GlobalFormId,
        item: GlobalFormId,
    ) -> Option<WornState> {
        let inventory = &self.reference_ref(actor)?.inventory;
        let stacks = inventory.stacks(item);
        (!stacks.is_empty()).then(|| WornState {
            worn: stacks.iter().any(|stack| stack.extra(22).is_some()),
            worn_left: stacks.iter().any(|stack| stack.extra(23).is_some()),
        })
    }

    /// Read a setting through the same script override and winning GMST path
    /// used by the game runtime.
    #[must_use]
    pub fn commonlib_game_setting(&self, name: &str) -> Option<SettingValue> {
        match self.game_setting(name)? {
            Value::Bool(value) => Some(SettingValue::Bool(value)),
            Value::Int(value) => Some(SettingValue::Int(value)),
            Value::Float(value) => Some(SettingValue::Float(value)),
            Value::Str(value) => Some(SettingValue::String(value.to_string())),
            Value::None | Value::Object(_) | Value::Array(_) | Value::Struct(_) => None,
        }
    }

    /// Read the placed HAZD reference fields currently represented by the
    /// reference table. This does not simulate the hazard.
    pub fn commonlib_hazard(&mut self, reference: GlobalFormId) -> Option<HazardState> {
        let (base, position, enabled) = {
            let state = self.reference(reference)?;
            (state.base?, state.position, state.enabled)
        };
        if self.record(base)?.tag != soe_esm::Tag(*b"HAZD") {
            return None;
        }
        Some(HazardState {
            reference: self.shim_id(reference),
            base: self.shim_id(base),
            position: NiPoint3 {
                x: position[0],
                y: position[1],
                z: position[2],
            },
            enabled,
        })
    }

    /// Read the actor's currently represented high-process subset.
    #[must_use]
    pub fn commonlib_high_process(&self, actor: GlobalFormId) -> Option<HighProcessState> {
        let state = self.actor_ref(actor)?;
        Some(HighProcessState {
            actor: self.shim_id(actor),
            in_combat: state.in_combat,
            combat_target: state.combat_target.map(|id| self.shim_id(id)),
            staggered: self.is_staggered(actor),
        })
    }

    /// Resolve a host-confirmed melee contact and project the result as
    /// CommonLib HitData. The game combat path remains the only writer.
    pub fn commonlib_resolve_hit(
        &mut self,
        hit: MeleeHit,
        outgoing_perks: Vec<crate::perks::EntryPointInvocation>,
        incoming_perks: Vec<crate::perks::EntryPointInvocation>,
        alloc: crate::world::Alloc<'_>,
    ) -> Result<HitData, crate::combat::Error> {
        let result = self.melee_hit(hit, outgoing_perks, incoming_perks, alloc)?;
        Ok(self.commonlib_hit_data(hit, result))
    }

    fn commonlib_hit_data(&self, hit: MeleeHit, result: HitResult) -> HitData {
        HitData {
            attacker: self.shim_id(hit.attacker),
            target: self.shim_id(hit.target),
            weapon: self.shim_id(hit.weapon),
            weapon_damage: result.weapon_damage,
            armor_reduction: result.armor_reduction,
            health_damage: result.health_damage,
            blocked: result.blocked,
            killed: result.killed,
        }
    }
}

impl RuntimeState for World {
    fn setting(&self, name: &str) -> Option<Setting> {
        let value = match self.game_setting(name)? {
            Value::Bool(value) => RuntimeSettingValue::Bool(value),
            Value::Int(value) => RuntimeSettingValue::Integer(value),
            Value::Float(value) => RuntimeSettingValue::Float(value),
            Value::Str(value) => RuntimeSettingValue::String(value.to_string()),
            Value::None | Value::Object(_) | Value::Array(_) | Value::Struct(_) => return None,
        };
        Some(Setting {
            name: name.to_owned(),
            value,
        })
    }

    fn sky(&self) -> Sky {
        Sky {
            hour: self.calendar().hour as f32,
        }
    }

    fn reanimate_effect(&self, id: u64) -> Option<ReanimateEffect> {
        let state = self.active_effects.get(&super::magic::EffectId(id))?;
        if self.magic_effect(state.effect).ok()?.data.archetype != 22 {
            return None;
        }
        let packed = |form| FormId(self.order.runtime_id(form));
        Some(ReanimateEffect {
            id,
            spell: packed(state.spell),
            effect: packed(state.effect),
            target: packed(state.target),
            magnitude: state.magnitude,
            elapsed: state.elapsed,
            duration: state.duration,
        })
    }

    fn sit_sleep_state(&self, _actor: FormId) -> Result<SitSleepState, UnsupportedWorldHook> {
        Err(UnsupportedWorldHook {
            name: "SIT_SLEEP_STATE",
            reason: "actor sit and sleep animation state is not represented by the runtime",
        })
    }

    fn hook(&self, _name: &str) -> UnsupportedWorldHook {
        UnsupportedWorldHook {
            name: "binary hook",
            reason: "binary address and vtable hooks have no target in this engine",
        }
    }
}

/// Report the planned hooks with no corresponding engine behavior.
#[must_use]
pub fn unsupported_hook(type_name: &str, hook_name: &str) -> UnsupportedHook {
    world_types::unsupported_hook(type_name, hook_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::magic::{ActiveEffect, EffectId};
    use crate::world::{ActorState, RefState};
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId {
            file: soe_esm::CREATED_FILE,
            object,
        }
    }

    #[test]
    fn shm_11_game_settings_read_the_live_world_override() {
        let mut world = World::new(LoadOrder::new());
        world
            .settings
            .insert("fcombatdistance".to_owned(), Value::Float(96.0));
        assert_eq!(
            world.commonlib_game_setting("fCombatDistance"),
            Some(SettingValue::Float(96.0))
        );
        assert_eq!(world.commonlib_game_setting("missing"), None);
    }

    #[test]
    fn shm_11_high_process_reads_combat_target_and_stagger_state() {
        let actor = id(1);
        let target = id(2);
        let mut world = World::new(LoadOrder::new());
        world.actors.insert(
            actor,
            ActorState {
                in_combat: true,
                combat_target: Some(target),
                stagger_end_time: Some(world.clock.real_seconds + 1.0),
                ..ActorState::default()
            },
        );
        let process = world.commonlib_high_process(actor).unwrap();
        assert!(process.in_combat && process.staggered);
        assert_eq!(process.actor, FormId(0xff00_0001));
        assert_eq!(process.combat_target, Some(FormId(0xff00_0002)));
    }

    #[test]
    fn shm_11_extra_worn_hazard_and_hit_data_have_explicit_absence_paths() {
        let mut world = World::new(LoadOrder::new());
        let actor = id(1);
        let item = id(2);
        let mut reference = RefState::new(None);
        reference
            .inventory
            .add_selected_stack(
                item,
                crate::inventory::Stack::new(
                    1,
                    vec![crate::inventory::Extra {
                        kind: 22,
                        fields: Vec::new(),
                    }],
                )
                .unwrap(),
            )
            .unwrap();
        world.refs.insert(actor, reference);
        assert_eq!(
            world.commonlib_extra_worn(actor, item),
            Some(WornState {
                worn: true,
                worn_left: false
            })
        );
        assert_eq!(world.commonlib_hazard(id(3)), None);
        assert_eq!(
            unsupported_hook("HitData", "dispatch"),
            UnsupportedHook {
                type_name: "HitData",
                hook_name: "dispatch",
                reason: "hit event dispatch is owned by game combat execution",
            }
        );
    }
    #[test]
    fn shm_15_reanimation_view_reads_the_live_effect_and_rejects_other_archetypes() {
        let mut data = vec![0; 152];
        data[64..68].copy_from_slice(&22u32.to_le_bytes());
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"MGEF"), 0x200).sub(Tag(*b"DATA"), data));
        plugin.push_top(Record::new(Tag(*b"MGEF"), 0x201).sub(Tag(*b"DATA"), vec![0; 152]));
        let mut order = LoadOrder::new();
        order.add("fixture.esp", plugin).unwrap();
        let mut world = World::new(order);
        let spell = crate::world::GlobalFormId {
            file: 0,
            object: 0x300,
        };
        let effect = crate::world::GlobalFormId {
            file: 0,
            object: 0x200,
        };
        let target = crate::world::GlobalFormId {
            file: 0,
            object: 0x400,
        };
        world.active_effects.insert(
            EffectId(1),
            ActiveEffect {
                id: EffectId(1),
                spell,
                effect,
                effect_index: 0,
                source: spell,
                caster: None,
                target,
                magnitude: 10.0,
                duration: Some(8.0),
                elapsed: 2.0,
                summoned: None,
                restore_owner: None,
                recover: false,
                changes: Vec::new(),
            },
        );
        world.active_effects.insert(
            EffectId(2),
            ActiveEffect {
                id: EffectId(2),
                spell,
                effect: crate::world::GlobalFormId {
                    file: 0,
                    object: 0x201,
                },
                effect_index: 0,
                source: spell,
                caster: None,
                target,
                magnitude: 1.0,
                duration: None,
                elapsed: 0.0,
                summoned: None,
                restore_owner: None,
                recover: false,
                changes: Vec::new(),
            },
        );

        let live = world.reanimate_effect(1).unwrap();
        assert_eq!(live.target, FormId(world.order.runtime_id(target)));
        assert_eq!(live.magnitude, 10.0);
        assert_eq!(live.elapsed, 2.0);
        assert_eq!(live.duration, Some(8.0));
        assert_eq!(world.reanimate_effect(2), None);
        assert_eq!(world.reanimate_effect(3), None);
    }
}
