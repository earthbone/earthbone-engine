// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Judge check for RND-08 (execute NIF transform controllers for animated
//! statics). Planned check rnd_08: an authored transform controller moves
//! the target along its track and respects its cycle mode.
//!
//! Exercises `controller::evaluate_time` and `controller::evaluate_transform`
//! through their public struct surface, the same approach the writer's own
//! `controller_adversarial.rs` uses. Expected numbers for cycle mode are
//! computed by hand from OpenMW's `ControllerFunction::calculate`
//! (`components/nifosg/controller.cpp` lines 20-71, read from
//! `../engine-refs/openmw`), not by re-deriving the crate's own formula, so
//! the test can catch a wrong cycle-mode implementation rather than just
//! confirm it agrees with itself.
//!
//! Check test only. Writes no non-test source.

use soe_nif::controller::{
    evaluate_time, evaluate_transform, ControllerFlags, CycleType, Interpolation, Key, KeyGroup,
    QuatKey, RotationKeys, Tbc, TimeController, TransformData, TransformInterpolator,
};
use soe_nif::types::{BlockRef, Quat, Vec3};

/// Cycle math divides by `delta` and multiplies back, so a value that lands
/// exactly on a cycle boundary in exact arithmetic can come back a few ULPs
/// off in `f32`. Expected values below are computed by hand from the cited
/// OpenMW source, not from the crate's own formula, so this tolerance is
/// only absorbing floating point rounding, not hiding a wrong result.
fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-3,
        "{what}: expected {expected}, got {actual}"
    );
}

fn vkey(time: f32, value: Vec3) -> Key<Vec3> {
    Key {
        time,
        value,
        in_tan: Vec3::ZERO,
        out_tan: Vec3::ZERO,
        tbc: Tbc::default(),
    }
}

fn fkey(time: f32, value: f32) -> Key<f32> {
    Key {
        time,
        value,
        in_tan: 0.0,
        out_tan: 0.0,
        tbc: Tbc::default(),
    }
}

fn qkey(time: f32, value: Quat) -> QuatKey {
    QuatKey {
        time,
        value,
        tbc: Tbc::default(),
    }
}

/// A `TimeController` with unit frequency, no phase, over `[0, 10]`, so
/// `game_time` maps straight onto the local track time before cycle-mode
/// extrapolation runs.
fn time_controller(cycle_type: CycleType) -> TimeController {
    TimeController {
        next_controller: BlockRef::NONE,
        flags: ControllerFlags {
            cycle_type,
            active: true,
            ..Default::default()
        },
        frequency: 1.0,
        phase: 0.0,
        start_time: 0.0,
        stop_time: 10.0,
        target: BlockRef::NONE,
    }
}

/// A linear track moving from the origin to `(100, 0, 0)`, rotating a
/// quarter turn about Z, and scaling from 1x to 3x, over `[0, 10]`. Every
/// channel changes, so a bug that leaves any one of them at its default
/// cannot hide behind the other two.
fn moving_track() -> TransformData {
    let quarter_turn_z = Quat {
        w: std::f32::consts::FRAC_1_SQRT_2,
        x: 0.0,
        y: 0.0,
        z: std::f32::consts::FRAC_1_SQRT_2,
    };
    TransformData {
        rotations: RotationKeys::Quaternion {
            interpolation: Interpolation::Linear,
            keys: vec![qkey(0.0, Quat::IDENTITY), qkey(10.0, quarter_turn_z)],
        },
        translations: KeyGroup {
            interpolation: Interpolation::Linear,
            keys: vec![
                vkey(0.0, Vec3::ZERO),
                vkey(10.0, Vec3::new(100.0, 0.0, 0.0)),
            ],
        },
        scales: KeyGroup {
            interpolation: Interpolation::Linear,
            keys: vec![fkey(0.0, 1.0), fkey(10.0, 3.0)],
        },
    }
}

/// Clause 1 of the done check: the target moves along its track. Every
/// channel (translation, rotation, scale) must differ between the start
/// pose, a midpoint pose and the end pose, and translation and scale must
/// move monotonically since the track is a single linear span.
#[test]
fn rnd_08_authored_controller_moves_target_along_its_track() {
    let time = time_controller(CycleType::Clamp);
    let interp = TransformInterpolator::default();
    let data = moving_track();

    let start = evaluate_transform(&time, &interp, Some(&data), 0.0);
    let mid = evaluate_transform(&time, &interp, Some(&data), 5.0);
    let end = evaluate_transform(&time, &interp, Some(&data), 10.0);

    // Translation and scale are exact at the keys themselves, and their
    // midpoint on a two-key linear track is the arithmetic mean.
    assert_eq!(start.translation, Vec3::ZERO);
    assert_eq!(mid.translation, Vec3::new(50.0, 0.0, 0.0));
    assert_eq!(end.translation, Vec3::new(100.0, 0.0, 0.0));
    assert!(start.translation.x < mid.translation.x);
    assert!(mid.translation.x < end.translation.x);

    assert_eq!(start.scale, 1.0);
    assert_eq!(mid.scale, 2.0);
    assert_eq!(end.scale, 3.0);

    // Rotation: the start pose is the identity matrix, and the end pose
    // must actually rotate a vector, not sit at identity too.
    assert_eq!(start.rotation, soe_nif::Mat33::IDENTITY);
    let rotated = end.rotation.apply(Vec3::new(1.0, 0.0, 0.0));
    assert!(
        (rotated.x).abs() < 1e-4 && (rotated.y - 1.0).abs() < 1e-4,
        "quarter turn about Z should send +X to +Y, got {rotated:?}"
    );
    // The midpoint rotation must differ from both ends: the controller is
    // actually interpolating rotation, not holding a fixed pose.
    assert_ne!(mid.rotation, start.rotation);
    assert_ne!(mid.rotation, end.rotation);
}

/// Clause 2 of the done check, `CycleType::Clamp`: past `stop_time` the
/// controller holds the end pose rather than extrapolating or resetting.
/// `ControllerFunction::calculate`'s default (Constant/Clamp) branch clamps
/// `time` to `[mStartTime, mStopTime]` outright.
#[test]
fn rnd_08_clamp_cycle_holds_the_edge_pose_past_the_track() {
    let time = time_controller(CycleType::Clamp);
    assert_eq!(evaluate_time(&time, 25.0), 10.0);
    assert_eq!(evaluate_time(&time, -5.0), 0.0);

    let interp = TransformInterpolator::default();
    let data = moving_track();
    let past_end = evaluate_transform(&time, &interp, Some(&data), 25.0);
    let at_end = evaluate_transform(&time, &interp, Some(&data), 10.0);
    assert_eq!(past_end.translation, at_end.translation);
    assert_eq!(past_end.scale, at_end.scale);
}

/// Clause 2, `CycleType::Loop`: past `stop_time` the controller wraps back
/// into `[start_time, stop_time]`. `game_time = 13` is one full 10 second
/// cycle plus 3 seconds, so it must land on local time 3, computed by hand
/// from `ControllerFunction::calculate`'s Cycle branch, not by calling
/// `evaluate_time` a second time.
#[test]
fn rnd_08_loop_cycle_wraps_the_target_back_into_the_track() {
    let time = time_controller(CycleType::Loop);
    assert_close(evaluate_time(&time, 13.0), 3.0, "loop wrap of game_time 13");
    assert_eq!(evaluate_time(&time, 3.0), 3.0);

    let interp = TransformInterpolator::default();
    let data = moving_track();
    let looped = evaluate_transform(&time, &interp, Some(&data), 13.0);
    let direct = evaluate_transform(&time, &interp, Some(&data), 3.0);
    assert_close(
        looped.translation.x,
        direct.translation.x,
        "loop translation.x",
    );
    assert_close(looped.scale, direct.scale, "loop scale");
    // And it must actually differ from the held end pose: a loop is not a
    // clamp, the target keeps moving instead of sitting still.
    let at_end = evaluate_transform(&time, &interp, Some(&data), 10.0);
    assert_ne!(looped.translation, at_end.translation);
}

/// Clause 2, `CycleType::Reverse`: past `stop_time` the controller ping
/// pongs. `game_time = 13` is 1.3 cycles in, an odd cycle count, so
/// `ControllerFunction::calculate`'s Reverse branch returns
/// `mStopTime - remainder` = `10 - 3 = 7`, by hand from the same source.
#[test]
fn rnd_08_reverse_cycle_ping_pongs_the_target() {
    let time = time_controller(CycleType::Reverse);
    assert_close(
        evaluate_time(&time, 13.0),
        7.0,
        "reverse ping-pong of game_time 13",
    );

    let interp = TransformInterpolator::default();
    let data = moving_track();
    let reversed = evaluate_transform(&time, &interp, Some(&data), 13.0);
    let direct_at_7 = evaluate_transform(&time, &interp, Some(&data), 7.0);
    assert_close(
        reversed.translation.x,
        direct_at_7.translation.x,
        "reverse translation.x",
    );

    // A second full cycle (game_time = 23: 2.3 cycles, even count) must
    // return to the forward direction, landing back at local time 3.
    assert_close(evaluate_time(&time, 23.0), 3.0, "reverse second cycle wrap");
}
