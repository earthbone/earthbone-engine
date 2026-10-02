// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib event channels backed by the live world.

use soe_commonlib_shim::events::{
    DisarmedEvent, DragonSoulsGained, FormIdentity, LevelIncrease, LocationDiscovery,
    ScriptEventSourceHolder, TesGrabReleaseEvent, TesHarvestedEvent, TesHitEvent,
    TesObjectLoadedEvent, TesQuestStageEvent, UnsupportedHook,
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

    pub(crate) fn commonlib_grab_release(&mut self, reference: GlobalFormId, grabbed: bool) {
        let event = TesGrabReleaseEvent {
            reference: ObjectRefHandle(self.runtime_id(reference)),
            grabbed,
        };
        self.commonlib_events.grab_release().send(&event);
    }

    pub(crate) fn commonlib_harvested(
        &mut self,
        produce_item: GlobalFormId,
        harvester: GlobalFormId,
    ) {
        let event = TesHarvestedEvent {
            produce_item: FormId(self.runtime_id(produce_item)),
            harvester: ActorHandle(self.runtime_id(harvester)),
        };
        self.commonlib_events.harvested().send(&event);
    }

    pub(crate) fn commonlib_hit(&mut self, event: TesHitEvent) {
        self.commonlib_events.hit().send(&event);
    }

    pub(crate) fn commonlib_object_loaded(&mut self, reference: GlobalFormId, loaded: bool) {
        let event = TesObjectLoadedEvent {
            form_id: FormId(self.runtime_id(reference)),
            loaded,
        };
        self.commonlib_events.object_loaded().send(&event);
    }

    pub(crate) fn commonlib_quest_stage(&mut self, quest: GlobalFormId, stage: u16) {
        let event = TesQuestStageEvent {
            quest: FormId(self.runtime_id(quest)),
            stage,
            item_index: 0,
        };
        self.commonlib_events.quest_stage().send(&event);
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

fn identity(id: GlobalFormId) -> FormIdentity {
    FormIdentity((u64::from(id.file) << 32) | u64::from(id.object))
}
