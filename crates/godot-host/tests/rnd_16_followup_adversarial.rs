// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::lod_rebuild::{select_far_level, silhouette_error_pixels};

#[test]
fn rnd_16_invalid_projected_bounds_cannot_pass_the_pixel_threshold() {
    let error = silhouette_error_pixels([f32::NAN, 0.0, 100.0, 100.0], [0.0, 0.0, 100.0, 100.0]);
    assert!(error.is_nan(), "invalid bounds must remain invalid");
    assert_eq!(
        select_far_level([error, f32::NAN, f32::NAN, f32::NAN], 1.0),
        None
    );
}
