// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared gameplay sky inputs for the host sky controller.

use super::{Arc, DataSource, PlayerNode};

impl PlayerNode {
    pub(crate) fn sky_interior_for_host(&self) -> Option<crate::cell_lighting::CellLighting> {
        let cell = self.loaded_cell?;
        if let Some(game) = &self.game {
            crate::cell_lighting::cell_lighting(game.world().order(), cell)
        } else {
            crate::cell_lighting::cell_lighting(&self.data_source.as_ref()?.shared_order(), cell)
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
        if self.simulation.is_some() {
            return Ok(self
                .simulation_pair
                .as_ref()
                .and_then(|pair| pair.current.sky.clone()));
        }
        if let Some(game) = &mut self.game {
            game.world_mut().sky_frame(cell)
        } else if let Some(source) = &self.data_source {
            soe_game::World::from_shared_order(source.shared_order()).sky_frame(cell)
        } else {
            Ok(None)
        }
    }
}
