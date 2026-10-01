// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Playback boundary cases for HST-01.

use soe_godot_host::animated_static::{self, AnimatedController};
use soe_nif::controller::{
    ControllerFlags, CycleType, QuatTransform, TimeController, TransformInterpolator,
};
use soe_nif::types::{BlockRef, Quat, Vec3};

fn controller(cycle_type: CycleType, active: bool) -> AnimatedController {
    AnimatedController {
        time: TimeController {
            next_controller: BlockRef::NONE,
            flags: ControllerFlags {
                cycle_type,
                active,
                ..Default::default()
            },
            frequency: 1.0,
            phase: 0.0,
            start_time: 0.0,
            stop_time: 1.0,
            target: BlockRef::NONE,
        },
        interpolator: TransformInterpolator {
            default_value: QuatTransform {
                translation: Vec3::new(3.0, 4.0, 5.0),
                rotation: Quat::IDENTITY,
                scale: 1.0,
            },
            data: BlockRef::NONE,
        },
        data: None,
    }
}

#[test]
fn hst_01_inactive_controller_keeps_default_pose_after_long_advance() {
    let animation = controller(CycleType::Loop, false);
    assert!(!animated_static::is_active(&animation));
    let pose = animated_static::evaluate(&animation, 1000.0);
    assert_eq!(pose.translation, Vec3::new(3.0, 4.0, 5.0));
}

#[test]
fn hst_01_clamped_controller_holds_its_default_pose_past_stop_time() {
    let animation = controller(CycleType::Clamp, true);
    let pose = animated_static::evaluate(&animation, 1000.0);
    assert_eq!(pose.translation, Vec3::new(3.0, 4.0, 5.0));
}
