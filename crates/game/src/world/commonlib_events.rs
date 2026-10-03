// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib event channels backed by the live world.

use soe_commonlib_shim::events::{
    DisarmedEvent, DragonSoulsGained, FormIdentity, LevelIncrease, LocationDiscovery,
    ScriptEventSourceHolder, UnsupportedHook,
};
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

    /// Add a reference to the map and report its first discovery.
    pub fn add_to_map(&mut self, reference: GlobalFormId, fast_travel: bool) {
        let (newly_added, cell_id) = {
            let Some(state) = self.reference(reference) else {
                return;
            };
            let newly_added = !state.on_map;
            state.on_map = true;
            state.fast_travel = fast_travel;
            (newly_added, state.cell)
        };
        if newly_added {
            let worldspace = cell_id
                .and_then(|cell| self.record(cell).and_then(|cell| cell.placement.world))
                .zip(cell_id)
                .and_then(|(raw, cell)| self.resolve_from(cell, raw));
            self.commonlib_location_discovery(reference, worldspace);
        }
    }

    /// Binary patch hooks have no implementation in the source-level shim.
    pub fn reject_commonlib_hook(&self, name: &'static str) -> Result<(), UnsupportedHook> {
        soe_commonlib_shim::events::install_hook(UnsupportedHook::new(name))
    }
}

fn identity(id: GlobalFormId) -> FormIdentity {
    FormIdentity((u64::from(id.file) << 32) | u64::from(id.object))
}
