// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for RND-08 (execute NIF transform controllers for
//! animated statics). Targets `controller::evaluate_time` and
//! `controller::evaluate_transform` directly through their public struct
//! surface, no byte-level parsing needed since every field used here is
//! `pub`.
//!
//! Grounded on OpenMW's `components/nifosg/controller.cpp`
//! (`ControllerFunction::calculate`) and `components/nif/data.hpp`
//! (`ValueInterpolator::interpolate`, the float/Vec3 and Quat overloads),
//! read from `../engine-refs/openmw`, and on `nif.xml` (niftools) for
//! `CONST_KEY`'s own step-function description.
//!
//! Breaker only. Writes no non-test source.

use soe_nif::controller::{
    evaluate_time, evaluate_transform, ControllerFlags, CycleType, Interpolation, Key, KeyGroup,
    QuatKey, RotationKeys, Tbc, TimeController, TransformData, TransformInterpolator,
};
use soe_nif::types::{BlockRef, Quat, Vec3};

fn key<T>(time: f32, value: T) -> Key<T>
where
    T: Default + Copy,
{
    Key {
        time,
        value,
        in_tan: T::default(),
        out_tan: T::default(),
        tbc: Tbc::default(),
    }
}

fn quat_key(time: f32, value: Quat) -> QuatKey {
    QuatKey {
        time,
        value,
        tbc: Tbc::default(),
    }
}

/// A controller with unit frequency and no phase, so `evaluate_time`'s
/// output equals `game_time` before cycle-mode extrapolation is applied.
fn plain_time(start_time: f32, stop_time: f32, cycle_type: CycleType) -> TimeController {
    TimeController {
        next_controller: BlockRef::NONE,
        flags: ControllerFlags {
            cycle_type,
            active: true,
            ..Default::default()
        },
        frequency: 1.0,
        phase: 0.0,
        start_time,
        stop_time,
        target: BlockRef::NONE,
    }
}

/// `ControllerFunction::calculate`, `components/nifosg/controller.cpp` lines
/// 24-25 and 44-45: both the Cycle (our `Loop`) and Reverse branches guard
/// `delta <= 0` (an inverted or zero-width `[start_time, stop_time]`) by
/// returning `mStartTime` outright, before any of the cycle math that
/// divides by `delta` runs. A malformed or hand-edited NIF can carry
/// `stop_time <= start_time`, and the controller must still hold a stable
/// pose rather than let the degenerate range leak `game_time` straight
/// through.
#[test]
fn evaluate_time_loop_cycle_holds_start_when_range_is_degenerate() {
    let time = plain_time(10.0, 0.0, CycleType::Loop);
    assert_eq!(evaluate_time(&time, 3.0), 10.0);
    assert_eq!(evaluate_time(&time, 70.0), 10.0);
}

/// Same guard as above, exercised through the Reverse branch, which OpenMW
/// implements as a second, textually separate `delta <= 0` check
/// (`components/nifosg/controller.cpp` lines 44-45).
#[test]
fn evaluate_time_reverse_cycle_holds_start_when_range_is_degenerate() {
    let time = plain_time(10.0, 0.0, CycleType::Reverse);
    assert_eq!(evaluate_time(&time, 3.0), 10.0);
    assert_eq!(evaluate_time(&time, 70.0), 10.0);
}

/// `nif.xml`'s `KeyType` enum documents `CONST_KEY` (value 5, our
/// `Interpolation::Constant`) as a step function, and OpenMW's
/// `ValueInterpolator::interpolate` (`components/nif/data.hpp`) implements
/// that step as `frac > 0.5f ? b.mValue : a.mValue`: once the sample point
/// has passed the midpoint between two bracketing keys, the value must snap
/// to the later key, not hold the earlier one for the whole span. This
/// covers both `KeyGroup<Vec3>` (translation) and `KeyGroup<f32>` (scale),
/// which share the same `evaluate_track` implementation.
#[test]
fn evaluate_transform_constant_translation_and_scale_step_to_nearer_key() {
    let time = plain_time(0.0, 10.0, CycleType::Clamp);
    let interp = TransformInterpolator::default();
    let data = TransformData {
        rotations: RotationKeys::None,
        translations: KeyGroup {
            interpolation: Interpolation::Constant,
            keys: vec![
                key(0.0, Vec3::new(1.0, 0.0, 0.0)),
                key(10.0, Vec3::new(9.0, 0.0, 0.0)),
            ],
        },
        scales: KeyGroup {
            interpolation: Interpolation::Constant,
            keys: vec![key(0.0, 2.0), key(10.0, 8.0)],
        },
    };

    // game_time 7.0 is 70% of the way from the first key to the second, past
    // the midpoint, so the step function must have already snapped to the
    // second key's translation and scale.
    let pose = evaluate_transform(&time, &interp, Some(&data), 7.0);
    assert_eq!(pose.translation, Vec3::new(9.0, 0.0, 0.0));
    assert_eq!(pose.scale, 8.0);
}

/// The same `CONST_KEY` step-function requirement as above, but through
/// `evaluate_rotation`, a textually separate function from `evaluate_track`
/// that implements its own (independently buggy) `Constant` case for
/// quaternion tracks.
#[test]
fn evaluate_transform_constant_rotation_steps_to_nearer_key() {
    let time = plain_time(0.0, 10.0, CycleType::Clamp);
    let interp = TransformInterpolator::default();
    let first = Quat {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let second = Quat {
        w: 0.0,
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };
    let data = TransformData {
        rotations: RotationKeys::Quaternion {
            interpolation: Interpolation::Constant,
            keys: vec![quat_key(0.0, first), quat_key(10.0, second)],
        },
        translations: KeyGroup::default(),
        scales: KeyGroup::default(),
    };

    // Past the midpoint again, so the step function must report the second
    // key's rotation.
    let pose = evaluate_transform(&time, &interp, Some(&data), 7.0);
    assert_eq!(pose.rotation, second.to_mat33());
}
