// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-backed checks for RND-22 material edge behavior.

use soe_godot_host::{mesh::{EffectMaterial, MaterialDesc}, nif_effect};
use soe_nif::{
    blocks::{flags1, flags2, BlendFactor, TestFunc},
    mesh::{AlphaTest, Blend, EffectParams},
};

fn material() -> (MaterialDesc, EffectMaterial) {
    (
        MaterialDesc {
            shader_flags_1: flags1::ZBUFFER_TEST,
            shader_flags_2: flags2::ZBUFFER_WRITE,
            ..Default::default()
        },
        EffectMaterial {
            params: EffectParams::default(),
            blend: None,
            alpha_test: None,
            uv_offset: [0.125, 0.25],
            uv_scale: [2.0, 0.5],
        },
    )
}

#[test]
fn rnd_22_uv_scale_then_offset_preserves_authored_effect_mapping() {
    let (desc, effect) = material();
    let shader = nif_effect::shader(&desc, &effect);

    assert!(shader.contains("vec2 uv = UV * uv_scale + uv_offset;"));
}

#[test]
fn rnd_22_standard_alpha_blend_uses_source_texture_alpha_once() {
    let (desc, mut effect) = material();
    effect.blend = Some(Blend {
        src: BlendFactor::SrcAlpha,
        dst: BlendFactor::OneMinusSrcAlpha,
    });
    let shader = nif_effect::shader(&desc, &effect);

    assert!(shader.contains("float opacity = tint.a * angle_opacity * (false ? 1.0 : texel.a);"));
    assert!(shader.contains("ALPHA = opacity;"));
    assert!(!shader.contains("ALBEDO *= opacity"));
}

#[test]
fn rnd_22_alpha_test_is_applied_after_soft_and_palette_alpha() {
    let (mut desc, mut effect) = material();
    desc.shader_flags_1 |= flags1::SOFT_EFFECT | flags1::GREYSCALE_TO_PALETTE_ALPHA;
    effect.alpha_test = Some(AlphaTest {
        func: TestFunc::GreaterEqual,
        threshold: 0.5,
    });
    let shader = nif_effect::shader(&desc, &effect);
    let soft = shader.find("opacity *= fade;").unwrap();
    let palette = shader
        .find("opacity = texture(palette_texture, vec2(texel.a, opacity)).a;")
        .unwrap();
    let test = shader.find("if (!(opacity >= alpha_threshold))").unwrap();

    assert!(soft < palette && palette < test);
}

#[test]
fn rnd_22_each_axis_clamp_flag_disables_only_its_wrap_coordinate() {
    let (mut desc, effect) = material();
    let base = nif_effect::shader(&desc, &effect);
    assert!(base.contains("if (!wrap_uv.x) { uv.x = clamp(uv.x"));
    assert!(base.contains("if (!wrap_uv.y) { uv.y = clamp(uv.y"));

    desc.shader_flags_1 &= !flags1::ZBUFFER_TEST;
    let no_depth_test = nif_effect::shader(&desc, &effect);
    assert!(no_depth_test.contains("depth_test_disabled"));
}
