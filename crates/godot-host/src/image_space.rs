// SPDX-License-Identifier: GPL-3.0-or-later

//! Host evaluation of winning IMAD curves and base IMGS inputs. xEdit's IMAD
//! layout and Godot's Environment API define the available data and host controls.

use soe_game::world::image_space::{BaseImageSpace, ImageSpaceCurveSample, ImageSpaceLayer};
use soe_game::World;

/// Evaluated channels attached to one active modifier layer.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSpaceRenderLayer {
    pub modifier: soe_esm::GlobalFormId,
    pub strength: f32,
    pub channels: Vec<ImageSpaceCurveSample>,
}

/// Combined values sent to the host post-processing pass.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedModifiers {
    pub tint: [f32; 4],
    pub blur_radius: f32,
    pub double_vision: f32,
    pub depth_strength: f32,
    pub depth_distance: f32,
    pub depth_range: f32,
    pub bloom_threshold: f32,
    pub bloom_scale: f32,
    pub saturation: f32,
    pub brightness: f32,
    pub contrast: f32,
}

impl Default for RenderedModifiers {
    fn default() -> Self {
        Self {
            tint: [0.0; 4],
            blur_radius: 0.0,
            double_vision: 0.0,
            depth_strength: 0.0,
            depth_distance: 0.0,
            depth_range: 1.0,
            bloom_threshold: 0.7,
            bloom_scale: 0.0,
            saturation: 1.0,
            brightness: 1.0,
            contrast: 1.0,
        }
    }
}

impl RenderedModifiers {
    /// Compose authored IMAD samples by multiplying each contribution by its
    /// active stack strength and adding it in application order.
    pub fn from_layers(layers: &[ImageSpaceRenderLayer]) -> Self {
        let mut rendered = Self::default();
        for layer in layers {
            let weight = layer.strength;
            for sample in &layer.channels {
                let first = sample.values.first().copied().unwrap_or_default() * weight;
                match &sample.channel {
                    b"TNAM" => {
                        for (target, value) in rendered.tint.iter_mut().zip(&sample.values) {
                            *target += value * weight;
                        }
                    }
                    b"BNAM" => rendered.blur_radius += first,
                    b"VNAM" => rendered.double_vision += first,
                    b"WNAM" => rendered.depth_strength += first,
                    b"XNAM" => {
                        rendered.depth_distance += first / soe_game::world::UNITS_PER_METRE;
                    }
                    b"YNAM" => {
                        rendered.depth_range += first / soe_game::world::UNITS_PER_METRE;
                    }
                    b"\x02IAD" => rendered.bloom_threshold += first - 0.7 * weight,
                    b"BIAD" => rendered.bloom_threshold += first,
                    b"\x03IAD" | b"CIAD" => rendered.bloom_scale += first,
                    b"\x11IAD" => rendered.saturation += first - weight,
                    b"QIAD" => rendered.saturation += first,
                    b"\x12IAD" => rendered.brightness += first - weight,
                    b"RIAD" => rendered.brightness += first,
                    b"\x13IAD" => rendered.contrast += first - weight,
                    b"SIAD" => rendered.contrast += first,
                    _ => {}
                }
            }
        }
        rendered.tint[3] = rendered.tint[3].clamp(0.0, 1.0);
        rendered.blur_radius = rendered.blur_radius.clamp(0.0, 16.0);
        rendered.double_vision = rendered.double_vision.clamp(0.0, 0.1);
        rendered.depth_strength = rendered.depth_strength.clamp(0.0, 1.0);
        rendered.depth_range = rendered.depth_range.max(0.01);
        rendered.bloom_threshold = rendered.bloom_threshold.max(0.0);
        rendered.bloom_scale = rendered.bloom_scale.max(0.0);
        rendered.saturation = rendered.saturation.max(0.0);
        rendered.brightness = rendered.brightness.max(0.0);
        rendered.contrast = rendered.contrast.max(0.0);
        rendered
    }
}

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
    use godot::classes::{
        Camera3D, CameraAttributesPractical, ColorRect, Environment, Shader, ShaderMaterial,
    };
    use godot::prelude::*;

    use super::{RenderedBase, RenderedModifiers};

    /// Camera-owned Godot resources. Dropping them restores the world's environment.
    #[derive(Default)]
    pub struct BaseRenderer {
        environment: Option<Gd<Environment>>,
        attributes: Option<Gd<CameraAttributesPractical>>,
    }

    /// Full screen color pass for active modifier layers.
    #[derive(Default)]
    pub struct ModifierRenderer {
        overlay: Option<Gd<ColorRect>>,
        material: Option<Gd<ShaderMaterial>>,
    }

    impl ModifierRenderer {
        /// Create the full screen overlay under the supplied CanvasLayer.
        pub fn attach(&mut self, layer: &mut Gd<godot::classes::CanvasLayer>) {
            let mut shader = Shader::new_gd();
            shader.set_code(include_str!("../../../godot/mag_71_image_space.gdshader"));
            let mut material = ShaderMaterial::new_gd();
            material.set_shader(&shader);
            let mut overlay = ColorRect::new_alloc();
            overlay.set_name("ImageSpaceModifierPass");
            overlay.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
            overlay.set_material(&material);
            overlay.hide();
            layer.add_child(&overlay);
            overlay
                .set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
            self.overlay = Some(overlay);
            self.material = Some(material);
        }

        /// Apply composed modifier values or hide the pass when no curves are active.
        pub fn apply(&mut self, values: &RenderedModifiers) {
            let Some(material) = &mut self.material else {
                return;
            };
            let enabled = values.tint[3] > 0.0
                || values.blur_radius > 0.0
                || values.double_vision > 0.0
                || values.bloom_scale > 0.0
                || values.saturation != 1.0
                || values.brightness != 1.0
                || values.contrast != 1.0;
            material.set_shader_parameter(
                "tint_color",
                &godot::builtin::Color::from_rgba(
                    values.tint[0],
                    values.tint[1],
                    values.tint[2],
                    values.tint[3],
                )
                .to_variant(),
            );
            material.set_shader_parameter("blur_radius", &values.blur_radius.to_variant());
            material.set_shader_parameter("double_vision", &values.double_vision.to_variant());
            material.set_shader_parameter("bloom_threshold", &values.bloom_threshold.to_variant());
            material.set_shader_parameter("bloom_scale", &values.bloom_scale.to_variant());
            material.set_shader_parameter("saturation", &values.saturation.to_variant());
            material.set_shader_parameter("brightness", &values.brightness.to_variant());
            material.set_shader_parameter("contrast", &values.contrast.to_variant());
            if let Some(overlay) = &mut self.overlay {
                if enabled {
                    overlay.show();
                } else {
                    overlay.hide();
                }
            }
        }
    }

    impl BaseRenderer {
        pub fn apply(
            &mut self,
            camera: &mut Gd<Camera3D>,
            values: Option<&RenderedBase>,
            modifiers: &RenderedModifiers,
        ) {
            if values.is_none() && modifiers.depth_strength == 0.0 {
                camera.set_environment(None::<&Gd<Environment>>);
                camera.set_attributes(None::<&Gd<CameraAttributesPractical>>);
                self.environment = None;
                self.attributes = None;
                return;
            }
            let neutral = RenderedBase {
                exposure: 1.0,
                white: 1.0,
                bloom: 0.0,
                saturation: 1.0,
                brightness: 1.0,
                contrast: 1.0,
                tint: [0.0; 4],
                depth_amount: 0.0,
                depth_distance: 0.0,
                depth_range: 1.0,
            };
            let values = values.unwrap_or(&neutral);
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
            let modifier_depth = modifiers.depth_strength > 0.0;
            let depth_amount = (values.depth_amount + modifiers.depth_strength).clamp(0.0, 1.0);
            let depth_distance = if modifier_depth {
                modifiers.depth_distance
            } else {
                values.depth_distance
            };
            let depth_range = if modifier_depth {
                modifiers.depth_range
            } else {
                values.depth_range
            };
            attributes.set(
                "dof_blur_far_enabled",
                &(depth_enabled || modifier_depth).to_variant(),
            );
            attributes.set("dof_blur_amount", &depth_amount.to_variant());
            attributes.set("dof_blur_far_distance", &depth_distance.to_variant());
            attributes.set("dof_blur_far_transition", &depth_range.to_variant());
            camera.set_attributes(Some(&*attributes));
        }
    }
}
