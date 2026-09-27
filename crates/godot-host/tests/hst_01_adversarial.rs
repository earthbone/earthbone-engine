// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-defined edge cases for animated static controller playback.

use soe_godot_host::animated_static::{self, AnimatedController};
use soe_nif::controller::{
    ControllerFlags, CycleType, QuatTransform, TimeController, TransformInterpolator,
};
use soe_nif::types::BlockRef;

#[test]
fn hst_01_controller_without_key_data_holds_its_authored_default_pose() {
    let default_value = QuatTransform {
        translation: soe_nif::types::Vec3::new(4.0, -2.0, 9.0),
        rotation: soe_nif::types::Quat::IDENTITY,
        scale: 1.5,
    };
    let controller = AnimatedController {
        time: TimeController {
            next_controller: BlockRef::NONE,
            flags: ControllerFlags {
                cycle_type: CycleType::Clamp,
                active: true,
                ..Default::default()
            },
            frequency: 1.0,
            phase: 0.0,
            start_time: 0.0,
            stop_time: 1.0,
            target: BlockRef::NONE,
        },
        interpolator: TransformInterpolator {
            default_value,
            data: BlockRef::NONE,
        },
        data: None,
    };

    assert_eq!(
        animated_static::evaluate(&controller, 0.0).translation,
        default_value.translation
    );
    assert_eq!(
        animated_static::evaluate(&controller, 25.0).translation,
        default_value.translation
    );
    assert_eq!(
        animated_static::evaluate(&controller, 25.0).scale,
        default_value.scale
    );
}
