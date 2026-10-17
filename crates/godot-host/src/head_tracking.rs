// SPDX-License-Identifier: GPL-3.0-or-later

//! Look correction after animation, following godotwind's look_at_modifier.gd.
//! Havok model transforms use Skyrim's positive Y forward and positive Z up.
//! Existing pose buffers replace godotwind's newer SkeletonModifier3D API.

use soe_hkx::{
    skeleton::{compose, Skeleton},
    QsTransform,
};

fn rotation(q: [f32; 4]) -> QsTransform {
    QsTransform {
        rotation: q,
        ..QsTransform::IDENTITY
    }
}

fn conjugate(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}

/// Aim the animated head and its descendants at a skeleton-space point.
/// The head pivot, other bones and authored scale remain in place.
pub(crate) fn aim(
    skeleton: &Skeleton,
    model: &mut [QsTransform],
    reference_head: [f32; 4],
    target: [f32; 3],
) -> Option<[f32; 2]> {
    if !target.iter().all(|value| value.is_finite()) {
        return None;
    }
    let head = skeleton.bone_index("NPC Head [Head]")?;
    let pose = *model.get(head)?;
    let direction = [
        target[0] - pose.translation[0],
        target[1] - pose.translation[1],
        target[2] - pose.translation[2],
    ];
    let horizontal = direction[0].hypot(direction[1]);
    if horizontal.hypot(direction[2]) < 0.001 {
        return None;
    }
    let yaw = direction[0]
        .atan2(direction[1])
        .clamp(-60f32.to_radians(), 60f32.to_radians());
    let pitch = direction[2]
        .atan2(horizontal)
        .clamp(-40f32.to_radians(), 40f32.to_radians());
    let (sin_yaw, cos_yaw) = (-yaw * 0.5).sin_cos();
    let (sin_pitch, cos_pitch) = (pitch * 0.5).sin_cos();
    let desired = compose(
        &rotation([0.0, 0.0, sin_yaw, cos_yaw]),
        &rotation([sin_pitch, 0.0, 0.0, cos_pitch]),
    );
    let current = compose(
        &rotation(pose.rotation),
        &rotation(conjugate(reference_head)),
    );
    let correction = compose(&desired, &rotation(conjugate(current.rotation)));
    for (index, bone_pose) in model.iter_mut().enumerate().skip(head) {
        let mut ancestor = index;
        let mut descendant = index == head;
        for _ in 0..skeleton.bones.len() {
            let Some(parent) = skeleton
                .parent_indices
                .get(ancestor)
                .copied()
                .and_then(|parent| usize::try_from(parent).ok())
            else {
                break;
            };
            if parent == head {
                descendant = true;
                break;
            }
            if parent >= ancestor {
                break;
            }
            ancestor = parent;
        }
        if descendant {
            let mut local = *bone_pose;
            for axis in 0..3 {
                local.translation[axis] -= pose.translation[axis];
            }
            let mut aimed = compose(&correction, &local);
            for axis in 0..3 {
                aimed.translation[axis] += pose.translation[axis];
            }
            *bone_pose = aimed;
        }
    }
    Some([yaw, pitch])
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_hkx::skeleton::Bone;

    fn rig() -> Skeleton {
        Skeleton {
            name: "look".into(),
            parent_indices: vec![-1, 0, 1, 0],
            bones: ["Root", "NPC Head [Head]", "Hair", "Hand"]
                .into_iter()
                .map(|name| Bone {
                    name: name.into(),
                    lock_translation: false,
                })
                .collect(),
            reference_pose: vec![QsTransform::IDENTITY; 4],
            reference_floats: vec![],
            float_slots: vec![],
        }
    }

    #[test]
    fn head_and_hair_turn_after_animation_without_moving_the_pivot_or_hand() {
        let skeleton = rig();
        let mut model = skeleton.reference_pose.clone();
        model[1].translation[2] = 100.0;
        model[2].translation = [0.0, 10.0, 100.0, 0.0];
        let pivot = model[1].translation;
        let hand = model[3];
        let angles = aim(
            &skeleton,
            &mut model,
            [0.0, 0.0, 0.0, 1.0],
            [100.0, 100.0, 100.0],
        )
        .unwrap();
        assert!((angles[0] - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
        assert_eq!(model[1].translation, pivot);
        assert_eq!(model[3], hand);
        assert!((model[2].translation[0] - 50f32.sqrt()).abs() < 1e-5);
        let forward = compose(
            &rotation(model[1].rotation),
            &QsTransform {
                translation: [0.0, 1.0, 0.0, 0.0],
                ..QsTransform::IDENTITY
            },
        );
        assert!((forward.translation[0] - 0.5f32.sqrt()).abs() < 1e-5);
    }

    #[test]
    fn look_limits_and_invalid_targets_preserve_finite_poses() {
        let skeleton = rig();
        let mut model = skeleton.reference_pose.clone();
        let angles = aim(
            &skeleton,
            &mut model,
            [0.0, 0.0, 0.0, 1.0],
            [1.0, -100.0, 1000.0],
        )
        .unwrap();
        assert_eq!(angles, [60f32.to_radians(), 40f32.to_radians()]);
        let before = model.clone();
        assert!(aim(
            &skeleton,
            &mut model,
            [0.0, 0.0, 0.0, 1.0],
            [f32::NAN, 0.0, 0.0]
        )
        .is_none());
        assert_eq!(model, before);
    }
}
