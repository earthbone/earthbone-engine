// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared gameplay sky inputs for the host sky controller.

use super::{Arc, DataSource, PlayerNode};

impl PlayerNode {
    pub(crate) fn sky_interior_for_host(&self) -> Option<soe_game::world::sky::InteriorLighting> {
        let cell = self.loaded_cell?;
        if let Some(game) = &self.game {
            game.world().interior_lighting(cell).ok()
        } else {
            soe_game::World::from_shared_order(self.data_source.as_ref()?.shared_order())
                .interior_lighting(cell)
                .ok()
        }
    }
    pub(crate) fn sky_source(&self) -> Option<Arc<DataSource>> {
        self.data_source.clone()
    }

    pub(crate) fn sky_frame_for_host(
        &mut self,
    ) -> Result<Option<soe_game::world::sky::SkyFrame>, soe_papyrus::VmError> {
        let Some(cell) = self.loaded_cell else {
            return Ok(None);
        };
        if let Some(game) = &mut self.game {
            game.world_mut().sky_frame(cell)
        } else if let Some(source) = &self.data_source {
            soe_game::World::from_shared_order(source.shared_order()).sky_frame(cell)
        } else {
            Ok(None)
        }
    }
}
