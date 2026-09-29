// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib world views over live state, following local CommonLibSSE-NG declarations.
//! Reanimation uses the MGEF archetype from `EffectArchetypes.h`.

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{
    ReanimateEffect, RuntimeState, Setting, SettingValue, SitSleepState, Sky, UnsupportedWorldHook,
};
use soe_papyrus::Value;

use super::World;

impl RuntimeState for World {
    fn setting(&self, name: &str) -> Option<Setting> {
        let value = match self.game_setting(name)? {
            Value::Bool(value) => SettingValue::Bool(value),
            Value::Int(value) => SettingValue::Integer(value),
            Value::Float(value) => SettingValue::Float(value),
            Value::Str(value) => SettingValue::String(value.to_string()),
            Value::None | Value::Object(_) | Value::Array(_) => return None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::magic::{ActiveEffect, EffectId};
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

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
