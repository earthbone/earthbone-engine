// SPDX-License-Identifier: GPL-3.0-or-later

//! Host view of the shared game's image-space lifetimes. Curve rendering is MAG-71.

use soe_game::world::image_space::ImageSpaceLayer;
use soe_game::World;

/// A derived frame snapshot. The shared World owns all lifetime changes.
#[derive(Default)]
pub struct ImageSpaceStack {
    layers: Vec<ImageSpaceLayer>,
}

impl ImageSpaceStack {
    pub fn sync(&mut self, world: &World) {
        self.layers = world.image_space_layers();
    }

    pub fn layers(&self) -> &[ImageSpaceLayer] {
        &self.layers
    }
}
