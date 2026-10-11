// SPDX-License-Identifier: GPL-3.0-or-later

//! RenderingServer object animation submission. Uses the same private
//! material and skeleton resources as the actor host, without scene nodes.

use super::*;
use crate::object_animation::{AnimatedSurface, ObjectAnimationClock};
use soe_nif::object_animation::{ObjectPose, PropertyChannel};

pub(super) struct RuntimeSurface {
    pub surface: AnimatedSurface,
    pub events: crate::animation_dispatch::AnimationEventCursor,
    clock: ObjectAnimationClock,
    sequence: Option<String>,
    material: Gd<Material>,
    skeleton: Option<Rid>,
    base: MaterialDesc,
    pub(super) last_transforms: Vec<Transform3D>,
}

impl Drop for RuntimeSurface {
    fn drop(&mut self) {
        if let Some(skeleton) = self.skeleton {
            RenderingServer::singleton().free_rid(skeleton);
        }
    }
}

impl RuntimeSurface {
    pub(super) fn play(&mut self, name: &str) -> bool {
        let Some(name) = self
            .surface
            .asset
            .sequence_names()
            .find(|candidate| candidate.eq_ignore_ascii_case(name))
        else {
            return false;
        };
        self.sequence = Some(name.to_owned());
        self.clock.reset();
        true
    }

    fn update(&mut self, rid: Rid, delta: f32) {
        let pose = self
            .clock
            .update(&self.surface.asset, delta, self.sequence.as_deref());
        let shape = pose
            .transforms
            .get(self.surface.block)
            .copied()
            .unwrap_or(soe_nif::Transform::IDENTITY);
        let transform = if self.skeleton.is_some() {
            self.surface.placement
        } else {
            self.surface.placement.then(&shape)
        };
        let transform = transform3d_from(placement_to_godot(
            transform.rotation.rows,
            transform.translation.to_array(),
            transform.scale,
        ));
        let mut server = RenderingServer::singleton();
        server.instance_set_transform(rid, transform);
        self.last_transforms.clear();
        self.last_transforms.push(transform);
        if let Some(skeleton) = self.skeleton {
            for (index, bone) in self.surface.skin_transforms(&pose).iter().enumerate() {
                let transform = Transform3D::new(
                    Basis::from_cols(
                        Vector3::new(bone.basis[0][0], bone.basis[1][0], bone.basis[2][0]),
                        Vector3::new(bone.basis[0][1], bone.basis[1][1], bone.basis[2][1]),
                        Vector3::new(bone.basis[0][2], bone.basis[1][2], bone.basis[2][2]),
                    ),
                    Vector3::new(bone.origin[0], bone.origin[1], bone.origin[2]),
                );
                server.skeleton_bone_set_transform(skeleton, index as i32, transform);
                self.last_transforms.push(transform);
            }
        }
        self.update_material(&pose);
    }

    fn update_material(&mut self, pose: &ObjectPose) {
        let desc = self.surface.material(&self.base, pose);
        let mut offset = self.surface.uv_offset;
        let mut scale = self.surface.uv_scale;
        let mut alpha = self.surface.alpha;
        let mut color = desc.emissive_color;
        let mut strength = desc.emissive_multiple;
        let mut uv_rotation = 0.0;
        if let Some(effect) = &desc.effect {
            offset = effect.uv_offset;
            scale = effect.uv_scale;
            alpha = effect.params.base_color[3];
            color.copy_from_slice(&effect.params.base_color[..3]);
            strength = effect.params.base_color_scale;
        }
        for value in &pose.properties {
            if Some(value.target) != self.surface.shader {
                continue;
            }
            match value.channel {
                PropertyChannel::Alpha => alpha = value.value.x,
                PropertyChannel::UOffset => offset[0] = value.value.x,
                PropertyChannel::VOffset => offset[1] = value.value.x,
                PropertyChannel::UScale => scale[0] = value.value.x,
                PropertyChannel::VScale => scale[1] = value.value.x,
                PropertyChannel::UvRotation => uv_rotation = value.value.x,
                _ => {}
            }
        }
        if let Ok(mut material) = self.material.clone().try_cast::<StandardMaterial3D>() {
            material.set_uv1_offset(Vector3::new(offset[0], offset[1], 0.0));
            material.set_uv1_scale(Vector3::new(scale[0], scale[1], 1.0));
            let albedo = material.get_albedo();
            material.set_albedo(Color::from_rgba(albedo.r, albedo.g, albedo.b, alpha));
            material.set_feature(Feature::EMISSION, true);
            material.set_emission(Color::from_rgb(color[0], color[1], color[2]));
            material.set_emission_energy_multiplier(strength);
        } else if let Ok(mut material) = self.material.clone().try_cast::<ShaderMaterial>() {
            material.set_shader_parameter(
                "uv_offset",
                &Vector2::new(offset[0], offset[1]).to_variant(),
            );
            material
                .set_shader_parameter("uv_scale", &Vector2::new(scale[0], scale[1]).to_variant());
            material.set_shader_parameter("uv_rotation", &uv_rotation.to_variant());
            if let Some(effect) = &desc.effect {
                material.set_shader_parameter(
                    "base_color",
                    &Vector4::new(color[0], color[1], color[2], alpha).to_variant(),
                );
                material.set_shader_parameter("base_scale", &strength.to_variant());
                let p = &effect.params;
                material.set_shader_parameter(
                    "falloff",
                    &Vector4::new(
                        p.falloff_start_angle,
                        p.falloff_stop_angle,
                        p.falloff_start_opacity,
                        p.falloff_stop_opacity,
                    )
                    .to_variant(),
                );
            } else {
                material.set_shader_parameter(
                    "emission_color",
                    &Color::from_rgb(color[0], color[1], color[2]).to_variant(),
                );
                material.set_shader_parameter("emission_strength", &strength.to_variant());
                material.set_shader_parameter("material_alpha", &alpha.to_variant());
            }
        }
    }
}

impl CellStreamerNode {
    pub(super) fn add_animated_surface(
        &mut self,
        cell: GlobalFormId,
        surface: &LoadedSurface,
        scenario: Rid,
    ) {
        let Some(animated) = surface.animation.clone() else {
            return;
        };
        let mut desc = surface.arrays.material.clone();
        let initial = animated.asset.sample(0.0, None);
        if desc.effect.is_none()
            && initial
                .properties
                .iter()
                .any(|p| Some(p.target) == animated.shader && p.channel == PropertyChannel::Alpha)
        {
            desc.alpha = AlphaMode::Blend;
        }
        let material = if let Some(effect) = &desc.effect {
            nif_effect_material(
                &desc,
                effect,
                &surface.textures,
                &mut self.materials.textures,
                &mut self.materials.shaders,
            )
            .upcast()
        } else {
            decal_material(
                &desc,
                &surface.textures,
                &mut self.materials.textures,
                &mut self.materials.cubemaps,
                &mut self.materials.shaders,
            )
            .upcast()
        };
        let table = BatchTable::Triangles(vec![BatchTableMember {
            reference: surface.reference,
            member: BatchMember {
                surface: 0,
                first_vertex: 0,
                vertex_count: surface.arrays.positions.len(),
                first_triangle: 0,
                triangle_count: surface.arrays.triangle_count(),
            },
            origin: surface.placement,
        }]);
        self.add_instance(cell, &surface.arrays, &material, scenario, table);
        let Some(instance) = self.instances.last_mut() else {
            return;
        };
        instance.bounds = animated.bounds(&surface.arrays, &initial);
        let mut server = RenderingServer::singleton();
        let skeleton = animated.skin.as_ref().map(|skin| {
            let skeleton = server.skeleton_create();
            server.skeleton_allocate_data(skeleton, skin.bones.len() as i32);
            server.instance_attach_skeleton(instance.rid, skeleton);
            skeleton
        });
        let mut runtime = RuntimeSurface {
            surface: animated,
            events: crate::animation_dispatch::AnimationEventCursor::beginning(),
            clock: ObjectAnimationClock::default(),
            sequence: None,
            material,
            skeleton,
            base: surface.arrays.material.clone(),
            last_transforms: Vec::new(),
        };
        runtime.update(instance.rid, 0.0);
        instance.animation = Some(runtime);
        self.bounds = self
            .instances
            .iter()
            .map(|i| i.bounds)
            .reduce(crate::batch::merge_bounds);
    }

    pub(super) fn advance_object_animations(&mut self, delta: f32) {
        for instance in &mut self.instances {
            if let Some(animation) = &mut instance.animation {
                animation.update(instance.rid, delta);
            }
        }
    }

    /// Start the activation sequence authored by this reference's NIF.
    pub(crate) fn activate_object_animation(&mut self, reference: GlobalFormId) -> bool {
        for name in [
            "Open", "Activate", "Trigger", "Pull", "Push", "Down", "On", "Forward", "Play",
        ] {
            if self.play_object_sequence(
                i32::from(reference.file),
                i64::from(reference.object),
                GString::from(name),
            ) > 0
            {
                return true;
            }
        }
        false
    }
}
