// SPDX-License-Identifier: GPL-3.0-or-later

//! `hkaSkeleton`, `hkaAnimationBinding` and the `hkaAnimationContainer` that
//! carries them. Offsets are the 64-bit `hk_2010.2.0-r1` layout.

use crate::animation::{Animation, SplineCompressedAnimation};
use crate::packfile::{root_variant_of, Obj};
use crate::{Error, Packfile, QsTransform, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub name: String,
    pub lock_translation: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    pub name: String,
    /// Parent bone index per bone, -1 for a root.
    pub parent_indices: Vec<i16>,
    pub bones: Vec<Bone>,
    /// Bind pose, one local transform per bone.
    pub reference_pose: Vec<QsTransform>,
    pub reference_floats: Vec<f32>,
    pub float_slots: Vec<String>,
}

impl Skeleton {
    /// Layout: name 16, parentIndices 24, bones 40 (hkaBone is 16 bytes:
    /// name 0, lockTranslation 8), referencePose 56, referenceFloats 72,
    /// floatSlots 88, localFrames 104.
    pub fn from_obj(o: Obj<'_>) -> Result<Skeleton> {
        o.expect_class("hkaSkeleton")?;
        let bones = o
            .array_structs(40, 16)?
            .into_iter()
            .map(|b| {
                Ok(Bone {
                    name: b.string_or_empty(0)?,
                    lock_translation: b.bool(8)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let skeleton = Skeleton {
            name: o.string_or_empty(16)?,
            parent_indices: o.array_i16(24)?,
            bones,
            reference_pose: o.array_qs_transform(56)?,
            reference_floats: o.array_f32(72)?,
            float_slots: o.array_strings(88)?,
        };
        if skeleton.parent_indices.len() != skeleton.bones.len()
            || skeleton.reference_pose.len() != skeleton.bones.len()
        {
            return Err(Error::BadSpline(format!(
                "skeleton {}: {} bones, {} parents, {} reference transforms",
                skeleton.name,
                skeleton.bones.len(),
                skeleton.parent_indices.len(),
                skeleton.reference_pose.len()
            )));
        }
        Ok(skeleton)
    }

    pub fn bone_index(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    /// Indices of the bones with no parent.
    pub fn roots(&self) -> Vec<usize> {
        self.parent_indices
            .iter()
            .enumerate()
            .filter(|(_, &p)| p < 0)
            .map(|(i, _)| i)
            .collect()
    }

    /// Model-space transforms for a local pose, walking parents. Bones are
    /// stored parent-first in Havok skeletons, and this checks that.
    pub fn to_model_space(&self, local: &[QsTransform]) -> Result<Vec<QsTransform>> {
        if local.len() != self.bones.len() {
            return Err(Error::BadSpline(format!(
                "pose has {} transforms for {} bones",
                local.len(),
                self.bones.len()
            )));
        }
        let mut out: Vec<QsTransform> = Vec::with_capacity(local.len());
        for (i, t) in local.iter().enumerate() {
            let p = self.parent_indices[i];
            if p < 0 {
                out.push(*t);
            } else if (p as usize) < i {
                out.push(compose(&out[p as usize], t));
            } else {
                return Err(Error::BadSpline(format!(
                    "bone {i} has parent {p} which comes later"
                )));
            }
        }
        Ok(out)
    }
}

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn quat_rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let p = [v[0], v[1], v[2], 0.0];
    let conj = [-q[0], -q[1], -q[2], q[3]];
    let r = quat_mul(quat_mul(q, p), conj);
    [r[0], r[1], r[2]]
}

/// `parent * child`: child expressed in the parent's space.
pub fn compose(parent: &QsTransform, child: &QsTransform) -> QsTransform {
    let scaled = [
        child.translation[0] * parent.scale[0],
        child.translation[1] * parent.scale[1],
        child.translation[2] * parent.scale[2],
    ];
    let rotated = quat_rotate(parent.rotation, scaled);
    QsTransform {
        translation: [
            parent.translation[0] + rotated[0],
            parent.translation[1] + rotated[1],
            parent.translation[2] + rotated[2],
            0.0,
        ],
        rotation: quat_mul(parent.rotation, child.rotation),
        scale: [
            parent.scale[0] * child.scale[0],
            parent.scale[1] * child.scale[1],
            parent.scale[2] * child.scale[2],
            0.0,
        ],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendHint {
    Normal,
    Additive,
    Other(i8),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationBinding {
    pub original_skeleton_name: String,
    /// Index into `AnimationContainer::animations`, when the binding's
    /// animation pointer is one of the container's.
    pub animation: Option<usize>,
    /// Data offset of the animation object, for lookups outside the container.
    pub animation_offset: Option<u32>,
    pub transform_track_to_bone_indices: Vec<i16>,
    pub float_track_to_float_slot_indices: Vec<i16>,
    pub blend_hint: BlendHint,
}

impl AnimationBinding {
    /// Layout: originalSkeletonName 16, animation 24,
    /// transformTrackToBoneIndices 32, floatTrackToFloatSlotIndices 48,
    /// blendHint 64.
    pub fn from_obj(o: Obj<'_>, animation_offsets: &[u32]) -> Result<AnimationBinding> {
        o.expect_class("hkaAnimationBinding")?;
        let animation_offset = o.ptr(24)?.map(|a| a.base);
        Ok(AnimationBinding {
            original_skeleton_name: o.string_or_empty(16)?,
            animation: animation_offset
                .and_then(|off| animation_offsets.iter().position(|&a| a == off)),
            animation_offset,
            transform_track_to_bone_indices: o.array_i16(32)?,
            float_track_to_float_slot_indices: o.array_i16(48)?,
            blend_hint: match o.i8(64)? {
                0 => BlendHint::Normal,
                1 => BlendHint::Additive,
                n => BlendHint::Other(n),
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationContainer {
    pub skeletons: Vec<Skeleton>,
    pub animations: Vec<Animation>,
    pub bindings: Vec<AnimationBinding>,
}

impl AnimationContainer {
    /// Layout: skeletons 16, animations 32, bindings 48, attachments 64,
    /// skins 80.
    pub fn from_obj(o: Obj<'_>) -> Result<AnimationContainer> {
        o.expect_class("hkaAnimationContainer")?;
        let skeletons = o
            .array_ptrs(16)?
            .into_iter()
            .flatten()
            .map(Skeleton::from_obj)
            .collect::<Result<Vec<_>>>()?;
        let anim_objs: Vec<Obj<'_>> = o.array_ptrs(32)?.into_iter().flatten().collect();
        let animation_offsets: Vec<u32> = anim_objs.iter().map(|a| a.base).collect();
        let animations = anim_objs
            .into_iter()
            .map(Animation::from_obj)
            .collect::<Result<Vec<_>>>()?;
        let bindings = o
            .array_ptrs(48)?
            .into_iter()
            .flatten()
            .map(|b| AnimationBinding::from_obj(b, &animation_offsets))
            .collect::<Result<Vec<_>>>()?;
        Ok(AnimationContainer {
            skeletons,
            animations,
            bindings,
        })
    }

    /// The container a file's root names, the usual entry point for
    /// `skeleton.hkx` and animation files.
    pub fn from_packfile(pf: &Packfile) -> Result<AnimationContainer> {
        pf.require_64bit()?;
        Self::from_obj(root_variant_of(pf, "hkaAnimationContainer")?)
    }

    pub fn spline_animations(&self) -> impl Iterator<Item = &SplineCompressedAnimation> {
        self.animations.iter().filter_map(|a| match a {
            Animation::SplineCompressed(s) => Some(s),
            Animation::Unsupported { .. } => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_1_SQRT_2;

    #[test]
    fn composing_with_the_identity_changes_nothing() {
        let t = QsTransform {
            translation: [1.0, 2.0, 3.0, 0.0],
            rotation: [0.0, 0.0, FRAC_1_SQRT_2, FRAC_1_SQRT_2],
            scale: [1.0, 1.0, 1.0, 0.0],
        };
        let c = compose(&QsTransform::IDENTITY, &t);
        assert_eq!(c.translation, t.translation);
        assert!((c.rotation[2] - t.rotation[2]).abs() < 1e-6);
    }

    #[test]
    fn a_child_is_rotated_by_its_parent() {
        let parent = QsTransform {
            translation: [0.0, 0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, FRAC_1_SQRT_2, FRAC_1_SQRT_2], // 90 degrees about z
            scale: [1.0, 1.0, 1.0, 0.0],
        };
        let child = QsTransform {
            translation: [1.0, 0.0, 0.0, 0.0],
            ..QsTransform::IDENTITY
        };
        let c = compose(&parent, &child);
        assert!(c.translation[0].abs() < 1e-5);
        assert!((c.translation[1] - 1.0).abs() < 1e-5);
    }
}
