// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib event channels backed by the live world.

use soe_commonlib_shim::events::{
    DisarmedEvent, DragonSoulsGained, EventState, FormIdentity, FurnitureEvent, FurnitureEventKind,
    LevelIncrease, LocationDiscovery, ScriptEventSourceHolder, UnsupportedHook,
};
use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::refr::{ActorHandle, ObjectRefHandle};
use soe_esm::GlobalFormId;

use super::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventError {
    LevelOutOfRange,
}

impl World {
    /// Access the CommonLib sources subscribed to by in-process plugins.
    pub fn commonlib_event_sources(&mut self) -> &mut ScriptEventSourceHolder {
        &mut self.commonlib_events
    }

    /// Emit the event produced by a successful disarm operation.
    pub fn commonlib_disarmed(&mut self, source: GlobalFormId, target: GlobalFormId) -> usize {
        self.commonlib_events.disarmed().send(&DisarmedEvent {
            source: identity(source),
            target: identity(target),
        })
    }

    /// Emit a dragon-soul award after the game state has accepted it.
    pub fn commonlib_dragon_souls_gained(&mut self, souls: f32) -> usize {
        self.commonlib_events
            .dragon_souls_gained()
            .send(&DragonSoulsGained { souls })
    }

    /// Emit the current player identity and level from live world state.
    pub fn commonlib_level_increase(&mut self) -> Result<usize, EventError> {
        let level = u16::try_from(self.player_level).map_err(|_| EventError::LevelOutOfRange)?;
        let player = self.player();
        Ok(self.commonlib_events.level_increase().send(&LevelIncrease {
            player: identity(player),
            new_level: level,
        }))
    }

    /// Emit a discovered marker and its owning worldspace identity.
    pub fn commonlib_location_discovery(
        &mut self,
        map_marker: GlobalFormId,
        worldspace: Option<GlobalFormId>,
    ) -> usize {
        self.commonlib_events
            .location_discovery()
            .send(&LocationDiscovery {
                map_marker: identity(map_marker),
                worldspace_id: worldspace.map(|id| identity(id).0.to_string()),
            })
    }

    /// Binary patch hooks have no implementation in the source-level shim.
    pub fn reject_commonlib_hook(&self, name: &'static str) -> Result<(), UnsupportedHook> {
        soe_commonlib_shim::events::install_hook(UnsupportedHook::new(name))
    }
}

impl EventState for World {
    fn inventory_count(&self, container: FormId, item: FormId) -> Option<i32> {
        let container = self.from_runtime_id(container.0)?;
        let item = self.from_runtime_id(item.0)?;
        self.reference_ref(container)
            .map(|reference| reference.inventory.count(item))
    }

    fn is_dead(&self, actor: ActorHandle) -> Option<bool> {
        let actor = self.from_runtime_id(actor.0)?;
        let reference = self.reference_ref(actor)?;
        if !reference.is_actor {
            return None;
        }
        self.actor_ref(actor).map(|state| state.dead)
    }

    fn is_in_bleedout(&self, actor: ActorHandle) -> Option<bool> {
        let actor = self.from_runtime_id(actor.0)?;
        let reference = self.reference_ref(actor)?;
        if !reference.is_actor {
            return None;
        }
        self.actor_ref(actor)
            .map(|state| state.unconscious && !state.dead)
    }

    fn is_form_deleted(&self, form: FormId) -> Option<bool> {
        let form = self.from_runtime_id(form.0)?;
        if !self.exists(form) {
            return None;
        }
        Some(
            self.reference_ref(form)
                .is_some_and(|reference| reference.deleted)
                || self.record(form).is_some_and(soe_esm::Record::is_deleted),
        )
    }

    fn furniture_event(&self, actor: ActorHandle) -> Option<FurnitureEvent> {
        let actor = self.from_runtime_id(actor.0)?;
        let actor_reference = self.reference_ref(actor)?;
        if !actor_reference.is_actor {
            return None;
        }
        let (furniture, _) = self.furniture_seat(actor)?;
        let furniture_reference = self.reference_ref(furniture)?;
        if furniture_reference.deleted || !furniture_reference.enabled {
            return None;
        }
        Some(FurnitureEvent {
            actor: ObjectRefHandle(self.runtime_id(actor)),
            target_furniture: ObjectRefHandle(self.runtime_id(furniture)),
            kind: FurnitureEventKind::Enter,
        })
    }
}

fn identity(id: GlobalFormId) -> FormIdentity {
    FormIdentity((u64::from(id.file) << 32) | u64::from(id.object))
}
