// SPDX-License-Identifier: GPL-3.0-or-later

//! NIF object poses and private material state, using nifly controller fields
//! and the existing host skin transform convention from skinned.rs.

use std::sync::Arc;

use soe_nif::object_animation::{ObjectAnimation, ObjectPose, PropertyChannel};
use soe_nif::{mesh::Skin, Transform};

use crate::mesh::MaterialDesc;
use crate::skinned::Affine;

/// One shape sharing an immutable NIF animation asset with sibling shapes.
#[derive(Debug, Clone)]
pub struct AnimatedSurface {
    pub asset: Arc<ObjectAnimation>,
    pub block: usize,
    pub shader: Option<usize>,
    pub skin: Option<Skin>,
    pub placement: Transform,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub alpha: f32,
}

impl AnimatedSurface {
    /// Bounds of the initial rendered pose, including skinning and placement.
    pub fn bounds(
        &self,
        arrays: &crate::mesh::SurfaceArrays,
        pose: &ObjectPose,
    ) -> crate::mesh::Aabb {
        let bones = self.skin_transforms(pose);
        let shape = pose
            .transforms
            .get(self.block)
            .copied()
            .unwrap_or(Transform::IDENTITY);
        let transform = if self.skin.is_some() {
            self.placement
        } else {
            self.placement.then(&shape)
        };
        let transform = Affine::from_nif(&transform).to_godot();
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for (index, point) in arrays.positions.iter().enumerate() {
            let mut value = *point;
            if !bones.is_empty() && arrays.is_skinned() {
                value = [0.0; 3];
                for slot in 0..4 {
                    let offset = index * 4 + slot;
                    let bone = arrays
                        .bones
                        .get(offset)
                        .and_then(|b| usize::try_from(*b).ok())
                        .and_then(|i| bones.get(i));
                    if let (Some(bone), Some(weight)) = (bone, arrays.weights.get(offset)) {
                        let transformed = bone.apply(*point);
                        for axis in 0..3 {
                            value[axis] += transformed[axis] * weight;
                        }
                    }
                }
            }
            let value = transform.apply(value);
            for axis in 0..3 {
                min[axis] = min[axis].min(value[axis]);
                max[axis] = max[axis].max(value[axis]);
            }
        }
        crate::mesh::Aabb {
            min: crate::units::GodotPos::new(min[0], min[1], min[2]),
            max: crate::units::GodotPos::new(max[0], max[1], max[2]),
        }
    }

    /// Final skin transforms in Godot space, indexed in the mesh bone order.
    pub fn skin_transforms(&self, pose: &ObjectPose) -> Vec<Affine> {
        self.skin.as_ref().map_or_else(Vec::new, |skin| {
            skin.bones
                .iter()
                .map(|bone| {
                    bone.node.get().and_then(|i| pose.transforms.get(i)).map_or(
                        Affine::IDENTITY,
                        |model| {
                            Affine::from_nif(model)
                                .then(&Affine::from_nif_skin(&bone.inverse_bind))
                                .to_godot()
                        },
                    )
                })
                .collect()
        })
    }

    /// Apply sampled shader fields to a fresh copy of the authored material.
    pub fn material(&self, base: &MaterialDesc, pose: &ObjectPose) -> MaterialDesc {
        let mut desc = base.clone();
        for property in &pose.properties {
            if Some(property.target) != self.shader {
                continue;
            }
            use PropertyChannel as P;
            let value = property.value.x;
            match property.channel {
                P::EmissiveColor => desc.emissive_color = property.value.to_array(),
                P::EmissiveMultiple => desc.emissive_multiple = value,
                _ => {}
            }
            if let Some(effect) = &mut desc.effect {
                match property.channel {
                    P::Alpha => effect.params.base_color[3] = value,
                    P::EmissiveMultiple => effect.params.base_color_scale = value,
                    P::EmissiveColor => {
                        effect.params.base_color[..3].copy_from_slice(&property.value.to_array());
                    }
                    P::UOffset => effect.uv_offset[0] = value,
                    P::VOffset => effect.uv_offset[1] = value,
                    P::UScale => effect.uv_scale[0] = value,
                    P::VScale => effect.uv_scale[1] = value,
                    P::FalloffStartAngle => effect.params.falloff_start_angle = value,
                    P::FalloffStopAngle => effect.params.falloff_stop_angle = value,
                    P::FalloffStartOpacity => effect.params.falloff_start_opacity = value,
                    P::FalloffStopOpacity => effect.params.falloff_stop_opacity = value,
                    _ => {}
                }
            }
        }
        desc
    }
}
