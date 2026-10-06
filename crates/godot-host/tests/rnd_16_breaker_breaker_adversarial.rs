// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for RND-16 projected-error selection.

use soe_godot_host::lod_rebuild::select_far_level;
use soe_godot_host::terrain_lod::LodLevel;

#[test]
fn rnd_16_negative_projected_error_is_not_accepted_as_valid() {
    // Pixel displacement is a magnitude and cannot be negative.
    assert_eq!(
        select_far_level([0.2, 0.4, -0.1, f32::NAN], 1.0),
        LodLevel::L8
    );
}

#[test]
fn rnd_16_selected_level_stays_within_the_pixel_limit_when_no_level_fits() {
    let errors = [1.2, 1.4, 1.8, 2.0];
    let selected = select_far_level(errors, 1.0);
    let error = match selected {
        LodLevel::L4 => errors[0],
        LodLevel::L8 => errors[1],
        LodLevel::L16 => errors[2],
        LodLevel::L32 => errors[3],
    };
    assert!(error <= 1.0, "selected {selected:?} with {error} px error");
}
