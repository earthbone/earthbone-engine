// SPDX-License-Identifier: GPL-3.0-or-later

//! Host view of authored image-space curves and their active layer weights.

use soe_game::world::image_space::{ImageSpaceCurveSample, ImageSpaceLayer};
use soe_game::World;

/// Evaluated channels attached to one active modifier layer.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSpaceRenderLayer {
    pub modifier: soe_esm::GlobalFormId,
    pub strength: f32,
    pub channels: Vec<ImageSpaceCurveSample>,
}

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

    /// Evaluate active authored curves while retaining their independent stack weights.
    pub fn render_layers(&self, world: &World) -> Result<Vec<ImageSpaceRenderLayer>, String> {
        self.layers
            .iter()
            .map(|layer| {
                let time = world
                    .image_space_layer_time(layer)
                    .map_err(|error| error.to_string())?;
                Ok(ImageSpaceRenderLayer {
                    modifier: layer.modifier,
                    strength: layer.strength,
                    channels: world
                        .image_space_curve_samples(layer.modifier, time)
                        .map_err(|error| error.to_string())?,
                })
            })
            .collect()
    }
}
