// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-33 adversarial: `generated_controller_shape`'s own doc comment says a
//! non-finite or non-positive scale "falls back to 1.0 rather than producing
//! degenerate geometry" (`crates/physics/src/shape.rs`). That guard checks
//! the *input* scale only. The function then computes
//! `TEMPLATE_CAPSULE_HEIGHT * scale` and `TEMPLATE_CAPSULE_RADIUS * scale`
//! and scales the result again by `UNITS_PER_METER` (about 70), so a scale
//! that is itself finite and positive, just large, can still push the
//! *output* capsule's coordinates past `f32::MAX` into infinity: exactly the
//! degenerate geometry the doc comment says the function avoids.
//!
//! This is not a contrived input. `World::actor_controller_scale` computes
//! `race.height[sex] * npc.height` from two record floats that are checked
//! only for finiteness, never for magnitude
//! (`crates/game/src/race.rs`'s `Attributes::from_record`'s `field` closure,
//! `crates/game/src/npc.rs`'s `f32_at`). A corrupted or hostile RACE or NPC_
//! record with a huge but finite height field passes that check and reaches
//! `generated_controller_shape` unclamped.

use soe_physics::{generated_controller_shape, Shape};

#[test]
fn cmb_33_generated_capsule_stays_finite_for_a_large_but_finite_scale_adversarial() {
    for scale in [f32::MAX, 1e37_f32, 5e36_f32] {
        assert!(
            scale.is_finite() && scale > 0.0,
            "scale {scale} must itself pass the existing input guard, or this test proves nothing"
        );
        let Shape::Capsule { a, b, radius } = generated_controller_shape(scale) else {
            panic!("generated_controller_shape must return a capsule");
        };
        assert!(
            a.x.is_finite() && a.y.is_finite() && a.z.is_finite(),
            "scale {scale}: lower cap {a:?} must stay finite, not become degenerate geometry"
        );
        assert!(
            b.x.is_finite() && b.y.is_finite() && b.z.is_finite(),
            "scale {scale}: upper cap {b:?} must stay finite, not become degenerate geometry"
        );
        assert!(
            radius.is_finite(),
            "scale {scale}: radius {radius} must stay finite, not become degenerate geometry"
        );
    }
}
