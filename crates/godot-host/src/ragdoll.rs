// SPDX-License-Identifier: GPL-3.0-or-later

//! Host ragdoll bone mapping from the HKX low resolution rig to the animated rig.
//!
//! The body and bone indices come from `soe_hkx::ragdoll`, which reads the
//! `hkaRagdollInstance` described by serde-hkx and activeragdoll. This module
//! keeps that mapping independent of Godot so death transitions can be tested.

use soe_hkx::ragdoll::{BodyIndex, Ragdoll, Transform};
use soe_hkx::skeleton::Skeleton;

use crate::skinned::Affine;

/// Map every animated bone to the closest ancestor with a ragdoll body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RagdollBoneMap {
    pub animated_to_body: Vec<Option<BodyIndex>>,
    pub body_to_animated: Vec<Option<usize>>,
}

impl RagdollBoneMap {
    /// Match names across the two HKX skeletons, preserving authored body order.
    #[must_use]
    pub fn new(animated: &Skeleton, ragdoll: &Ragdoll) -> Self {
        Self::from_skeletons(
            animated,
            &ragdoll.skeleton,
            &ragdoll.bone_to_body,
            ragdoll.bodies.len(),
        )
    }

    fn from_skeletons(
        animated: &Skeleton,
        low: &Skeleton,
        bone_to_body: &[Option<BodyIndex>],
        body_count: usize,
    ) -> Self {
        let mut body_to_animated = vec![None; body_count];
        for (bone, body) in bone_to_body.iter().enumerate() {
            if let Some(body) = body {
                body_to_animated[body.0] = animated.bones.iter().position(|candidate| {
                    candidate
                        .name
                        .eq_ignore_ascii_case(ragdoll_bone_name(&low.bones[bone].name))
                });
            }
        }
        let animated_to_body = (0..animated.bones.len())
            .map(|mut bone| loop {
                let name = &animated.bones[bone].name;
                if let Some(body) = low
                    .bones
                    .iter()
                    .position(|candidate| {
                        ragdoll_bone_name(&candidate.name).eq_ignore_ascii_case(name)
                    })
                    .and_then(|index| bone_to_body[index])
                {
                    break Some(body);
                }
                let parent = animated.parent_indices[bone];
                if parent < 0 || parent as usize >= bone {
                    break None;
                }
                bone = parent as usize;
            })
            .collect();
        Self {
            animated_to_body,
            body_to_animated,
        }
    }
}

fn ragdoll_bone_name(name: &str) -> &str {
    name.strip_prefix("Ragdoll_").unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_hkx::skeleton::Bone;
    use soe_hkx::QsTransform;

    fn skeleton(names: &[&str], parents: &[i16]) -> Skeleton {
        Skeleton {
            name: "test".into(),
            parent_indices: parents.to_vec(),
            bones: names
                .iter()
                .map(|name| Bone {
                    name: (*name).into(),
                    lock_translation: false,
                })
                .collect(),
            reference_pose: vec![QsTransform::IDENTITY; names.len()],
            reference_floats: Vec::new(),
            float_slots: Vec::new(),
        }
    }

    #[test]
    fn cmb_19_death_maps_animated_descendants_to_ragdoll_bodies() {
        let animated = skeleton(&["Pelvis", "Spine", "Hand", "Finger"], &[-1, 0, 1, 2]);
        let low = skeleton(&["Ragdoll_PELVIS", "Ragdoll_HAND"], &[-1, 0]);
        let map = RagdollBoneMap::from_skeletons(
            &animated,
            &low,
            &[Some(BodyIndex(1)), Some(BodyIndex(0))],
            2,
        );
        assert_eq!(
            map.animated_to_body,
            vec![
                Some(BodyIndex(1)),
                Some(BodyIndex(1)),
                Some(BodyIndex(0)),
                Some(BodyIndex(0))
            ]
        );
        assert_eq!(map.body_to_animated, vec![Some(2), Some(0)]);
    }

    #[test]
    fn cmb_19_resurrection_selects_animated_controller_again() {
        let states = [false, true, true, false].map(ActorRenderMode::for_dead);
        assert_eq!(
            states,
            [
                ActorRenderMode::Animated,
                ActorRenderMode::Ragdoll,
                ActorRenderMode::Ragdoll,
                ActorRenderMode::Animated
            ]
        );
    }

    #[test]
    fn cmb_19_havok_joint_frame_preserves_column_major_rotation() {
        let frame = Transform {
            rotation: [
                [0.0, 1.0, 0.0, 0.0],
                [-1.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
            ],
            translation: [10.0, 20.0, 30.0, 1.0],
        };
        let converted = affine_from_havok(&frame);
        assert_eq!(converted.apply([1.0, 0.0, 0.0]), [10.0, 21.0, 30.0]);
    }
}

/// Turn a column major Havok rigid body or joint frame into a row major affine.
#[must_use]
pub fn affine_from_havok(t: &Transform) -> Affine {
    Affine {
        basis: [
            [t.rotation[0][0], t.rotation[1][0], t.rotation[2][0]],
            [t.rotation[0][1], t.rotation[1][1], t.rotation[2][1]],
            [t.rotation[0][2], t.rotation[1][2], t.rotation[2][2]],
        ],
        origin: [t.translation[0], t.translation[1], t.translation[2]],
    }
}

/// Which host representation owns an actor's collision and skin pose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActorRenderMode {
    #[default]
    Animated,
    Ragdoll,
}

impl ActorRenderMode {
    /// The World death flag selects the representation on every host tick.
    #[must_use]
    pub fn for_dead(dead: bool) -> Self {
        if dead {
            Self::Ragdoll
        } else {
            Self::Animated
        }
    }
}

/// Whether a bound actor should render with a live ragdoll.
pub(crate) const fn actor_uses_ragdoll(
    dead: bool,
    unconscious: bool,
    forced: Option<bool>,
) -> bool {
    match forced {
        Some(enabled) => enabled,
        None => dead || unconscious,
    }
}

#[cfg(test)]
mod cmb_38_tests {
    use super::actor_uses_ragdoll;

    #[test]
    fn cmb_38_explicit_ragdoll_mode_overrides_death_state() {
        assert!(actor_uses_ragdoll(false, false, Some(true)));
        assert!(!actor_uses_ragdoll(true, true, Some(false)));
        assert!(actor_uses_ragdoll(true, false, None));
        assert!(actor_uses_ragdoll(false, true, None));
        assert!(!actor_uses_ragdoll(false, false, None));
    }

    #[test]
    fn cmb_38_impulse_uses_the_skyrim_to_godot_axis_map() {
        assert_eq!(
            crate::units::direction_to_godot([2.0, 3.0, 4.0]),
            [2.0, 4.0, -3.0]
        );
    }
}
