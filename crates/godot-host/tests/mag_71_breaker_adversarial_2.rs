// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn mag_71_hdr_bloom_threshold_and_scale_remain_distinct_channels() {
    let renderer = include_str!("../src/image_space.rs");
    let shader = include_str!("../../../godot/mag_71_image_space.gdshader");

    assert!(renderer.contains("bloom_threshold"));
    assert!(renderer.contains("bloom_scale"));
    assert!(shader.contains("uniform float bloom_threshold"));
    assert!(shader.contains("source - vec3(bloom_threshold)"));
}

#[test]
fn mag_71_authored_bloom_threshold_changes_the_screen_pass_cutoff() {
    let shader = include_str!("../../../godot/mag_71_image_space.gdshader");

    let threshold_use = shader.find("source - vec3(bloom_threshold)");
    let bloom_use = shader.find("glow * bloom_scale");

    assert!(threshold_use.is_some());
    assert!(bloom_use.is_some());
    assert!(threshold_use < bloom_use);
}
