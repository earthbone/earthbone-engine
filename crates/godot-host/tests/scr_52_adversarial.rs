// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-52 source-contract checks for RFCT presentation and its installed-data probe.

#[test]
fn scr_52_renderer_applies_camera_and_orientation_flags() {
    let layer = include_str!("../src/godot_layer.rs");
    let renderer = layer
        .split("pub(crate) fn sync_visual_effects")
        .nth(1)
        .unwrap()
        .split("fn free_visual_effects")
        .next()
        .unwrap();

    assert!(
        renderer.contains("request.flags"),
        "RFCT DATA flags must reach host presentation"
    );
    assert!(
        renderer.contains("request.facing"),
        "rotate-to-face-target needs the optional facing reference"
    );
}

#[test]
fn scr_52_capture_probe_requires_pixels_at_the_effect_location() {
    let probe = include_str!("../../../godot/scr_52_visual_effect_test.gd");

    assert!(
        probe.contains("before_effect") || probe.contains("sample_effect"),
        "capture must isolate the RFCT visual from existing scene pixels"
    );
    assert!(
        probe.contains("selected.flags"),
        "capture should exercise source-defined RFCT placement flags"
    );
}
