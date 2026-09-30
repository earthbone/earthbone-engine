// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib MIT API views over the game's live world state.

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{
    AIProcess, ActivateParentID, ActiveEffect, ActorCriticalStage, ActorValueModifier,
    UnsupportedState, WorldState,
};
use soe_esm::GlobalFormId;

use crate::actor_values::actor_value_name;
use crate::ai::Procedure;
use crate::world::World;

/// A short-lived adapter over a live runtime world. Form IDs use the same
/// resolved runtime encoding exposed to CommonLib plugins.
pub struct CommonLibWorld<'a> {
    world: &'a mut World,
}

impl<'a> CommonLibWorld<'a> {
    pub fn new(world: &'a mut World) -> Self {
        Self { world }
    }

    fn resolve(&self, id: FormId) -> Result<GlobalFormId, UnsupportedState> {
        self.world
            .from_runtime_id(id.0)
            .ok_or(UnsupportedState::MissingReference)
    }
}

impl WorldState for CommonLibWorld<'_> {
    fn critical_stage(&mut self, actor: FormId) -> Result<ActorCriticalStage, UnsupportedState> {
        let actor = self.resolve(actor)?;
        let state = self
            .world
            .actor(actor)
            .ok_or(UnsupportedState::MissingActor)?;
        let value =
            u32::try_from(state.critical_stage).map_err(|_| UnsupportedState::MissingActor)?;
        ActorCriticalStage::try_from(value)
    }

    fn set_critical_stage(
        &mut self,
        actor: FormId,
        stage: ActorCriticalStage,
    ) -> Result<(), UnsupportedState> {
        let actor = self.resolve(actor)?;
        self.world
            .actor(actor)
            .ok_or(UnsupportedState::MissingActor)?
            .critical_stage = stage as i32;
        Ok(())
    }

    fn actor_value_modifier(
        &mut self,
        actor: FormId,
        actor_value: u32,
        modifier: ActorValueModifier,
    ) -> Result<f32, UnsupportedState> {
        let actor = self.resolve(actor)?;
        let name = actor_value_name(
            i32::try_from(actor_value).map_err(|_| UnsupportedState::MissingActor)?,
        )
        .ok_or(UnsupportedState::MissingActor)?;
        let actor = self
            .world
            .actor(actor)
            .ok_or(UnsupportedState::MissingActor)?;
        let value = actor.values.get(name);
        Ok(match modifier {
            ActorValueModifier::Permanent => value.permanent(),
            ActorValueModifier::Temporary => value.temporary(),
            ActorValueModifier::Damage => value.damage(),
        })
    }

    fn ai_process(&mut self, actor: FormId) -> Result<AIProcess, UnsupportedState> {
        let actor = self.resolve(actor)?;
        let actor = self
            .world
            .actor(actor)
            .ok_or(UnsupportedState::MissingActor)?;
        let procedure = match &actor.ai.procedure {
            Procedure::None => "None".to_owned(),
            Procedure::Travel => "Travel".to_owned(),
            Procedure::Sandbox => "Sandbox".to_owned(),
            Procedure::Sleep => "Sleep".to_owned(),
            Procedure::Sit => "Sit".to_owned(),
            Procedure::Patrol => "Patrol".to_owned(),
            Procedure::UseMagic => "UseMagic".to_owned(),
            Procedure::UseWeapon => "UseWeapon".to_owned(),
            Procedure::Wait { .. } => "Wait".to_owned(),
            Procedure::Shout => "Shout".to_owned(),
            Procedure::Unsupported(name) => format!("Unsupported:{name}"),
        };
        Ok(AIProcess {
            running_package: actor.ai.package.map(|id| FormId(self.world.runtime_id(id))),
            procedure,
        })
    }

    fn activate_parents(
        &mut self,
        reference: FormId,
    ) -> Result<Vec<ActivateParentID>, UnsupportedState> {
        let reference_id = self.resolve(reference)?;
        let Some(record) = self.world.record(reference_id) else {
            return Err(UnsupportedState::MissingReference);
        };
        record
            .all(soe_esm::Tag::new(b"XAPR"))
            .map(|bytes| {
                if bytes.len() != 8 {
                    return Err(UnsupportedState::Hook("malformed XAPR activation parent"));
                }
                let raw_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                let delay = f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                if !delay.is_finite() {
                    return Err(UnsupportedState::Hook("non-finite XAPR activation delay"));
                }
                let parent = self
                    .world
                    .resolve_from(reference_id, raw_id)
                    .ok_or(UnsupportedState::MissingReference)?;
                Ok(ActivateParentID {
                    ref_id: FormId(self.world.runtime_id(parent)),
                    activate_delay: delay,
                })
            })
            .collect()
    }

    fn active_effect(&self, id: u64) -> Result<ActiveEffect, UnsupportedState> {
        let effect = self
            .world
            .active_effects()
            .find(|effect| effect.id.0 == id)
            .ok_or(UnsupportedState::MissingActiveEffect)?;
        Ok(ActiveEffect {
            id,
            spell: FormId(self.world.runtime_id(effect.spell)),
            effect: FormId(self.world.runtime_id(effect.effect)),
            source: FormId(self.world.runtime_id(effect.source)),
            caster: effect.caster.map(|id| FormId(self.world.runtime_id(id))),
            target: FormId(self.world.runtime_id(effect.target)),
            magnitude: effect.magnitude,
            duration: effect.duration,
            elapsed: effect.elapsed,
        })
    }

    fn unsupported_hook(&self, name: &'static str) -> Result<(), UnsupportedState> {
        Err(UnsupportedState::Hook(name))
    }
}
