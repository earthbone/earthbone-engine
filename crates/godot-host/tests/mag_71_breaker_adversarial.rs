// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::GlobalFormId;
use soe_game::world::image_space::ImageSpaceCurveSample;
use soe_godot_host::image_space::{ImageSpaceRenderLayer, RenderedModifiers};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn mag_71_cinematic_add_curves_follow_their_authored_channels_and_weights() {
    let layer = ImageSpaceRenderLayer {
        modifier: id(1),
        strength: 0.5,
        channels: vec![
            ImageSpaceCurveSample {
                channel: *b"\x11IAD",
                values: vec![1.5],
            },
            ImageSpaceCurveSample {
                channel: *b"QIAD",
                values: vec![0.2],
            },
            ImageSpaceCurveSample {
                channel: *b"\x12IAD",
                values: vec![1.5],
            },
            ImageSpaceCurveSample {
                channel: *b"RIAD",
                values: vec![0.3],
            },
            ImageSpaceCurveSample {
                channel: *b"\x13IAD",
                values: vec![1.5],
            },
            ImageSpaceCurveSample {
                channel: *b"SIAD",
                values: vec![0.4],
            },
        ],
    };

    let rendered = RenderedModifiers::from_layers(&[layer]);

    assert!((rendered.saturation - 1.35).abs() < 0.00001);
    assert!((rendered.brightness - 1.4).abs() < 0.00001);
    assert!((rendered.contrast - 1.65).abs() < 0.00001);
}

#[test]
fn mag_71_bloom_threshold_curve_controls_the_shader_cutoff() {
    let shader = include_str!("../../../godot/mag_71_image_space.gdshader");

    assert!(shader.contains("uniform float bloom_threshold"));
    assert!(shader.contains("source - vec3(bloom_threshold)"));
}
