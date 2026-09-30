// SPDX-License-Identifier: GPL-3.0-or-later

//! Host view of winning base IMGS and shared modifier lifetimes. xEdit IMGS
//! and Godot Environment/CameraAttributesPractical define the inputs.

use soe_game::world::image_space::{BaseImageSpace, ImageSpaceLayer};
use soe_game::World;

/// A derived frame snapshot. The shared World owns all lifetime changes.
#[derive(Default)]
pub struct ImageSpaceStack {
    layers: Vec<ImageSpaceLayer>,
    base: Option<BaseImageSpace>,
}

impl ImageSpaceStack {
    pub fn sync(&mut self, world: &World) -> Result<(), String> {
        self.layers = world.image_space_layers();
        self.base = None;
        self.base = world
            .base_image_space()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn layers(&self) -> &[ImageSpaceLayer] {
        &self.layers
    }

    pub fn base(&self) -> Option<&BaseImageSpace> {
        self.base.as_ref()
    }
}

/// Host values computed from one resolved base record. Distances are metres.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedBase {
    pub exposure: f32,
    pub white: f32,
    pub bloom: f32,
    pub saturation: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub tint: [f32; 4],
    pub depth_amount: f32,
    pub depth_distance: f32,
    pub depth_range: f32,
}

impl From<&BaseImageSpace> for RenderedBase {
    fn from(base: &BaseImageSpace) -> Self {
        Self {
            exposure: base.hdr[8].max(0.0) + 1.0,
            white: base.hdr[5].max(0.01),
            bloom: base.hdr[3].max(0.0),
            saturation: base.cinematic[0].max(0.0),
            brightness: base.cinematic[1].max(0.0),
            contrast: base.cinematic[2].max(0.0),
            tint: base.tint,
            depth_amount: base.depth[0].clamp(0.0, 1.0),
            depth_distance: base.depth[1].max(0.0) / soe_game::world::UNITS_PER_METRE,
            depth_range: base.depth[2].max(0.01) / soe_game::world::UNITS_PER_METRE,
        }
    }
}

#[cfg(feature = "godot")]
pub mod renderer {
    use godot::classes::{Camera3D, CameraAttributesPractical, Environment};
    use godot::prelude::*;

    use super::RenderedBase;

    /// Camera-owned Godot resources. Dropping them restores the world's environment.
    #[derive(Default)]
    pub struct BaseRenderer {
        environment: Option<Gd<Environment>>,
        attributes: Option<Gd<CameraAttributesPractical>>,
    }

    impl BaseRenderer {
        pub fn apply(&mut self, camera: &mut Gd<Camera3D>, values: Option<&RenderedBase>) {
            let Some(values) = values else {
                camera.set_environment(None::<&Gd<Environment>>);
                camera.set_attributes(None::<&Gd<CameraAttributesPractical>>);
                self.environment = None;
                self.attributes = None;
                return;
            };
            let environment = self.environment.get_or_insert_with(|| {
                camera
                    .get_world_3d()
                    .and_then(|world| world.get_environment())
                    .map(|base| base.duplicate_resource())
                    .unwrap_or_else(Environment::new_gd)
            });
            environment.set("adjustment_enabled", &true.to_variant());
            environment.set("adjustment_saturation", &values.saturation.to_variant());
            environment.set("adjustment_brightness", &values.brightness.to_variant());
            environment.set("adjustment_contrast", &values.contrast.to_variant());
            environment.set("tonemap_exposure", &values.exposure.to_variant());
            environment.set("tonemap_white", &values.white.to_variant());
            environment.set("glow_enabled", &(values.bloom > 0.0).to_variant());
            environment.set("glow_intensity", &values.bloom.to_variant());
            camera.set_environment(Some(&*environment));

            let attributes = self
                .attributes
                .get_or_insert_with(CameraAttributesPractical::new_gd);
            let depth_enabled = values.depth_amount > 0.0;
            attributes.set("dof_blur_far_enabled", &depth_enabled.to_variant());
            attributes.set("dof_blur_amount", &values.depth_amount.to_variant());
            attributes.set("dof_blur_far_distance", &values.depth_distance.to_variant());
            attributes.set("dof_blur_far_transition", &values.depth_range.to_variant());
            camera.set_attributes(Some(&*attributes));
        }
    }
}
