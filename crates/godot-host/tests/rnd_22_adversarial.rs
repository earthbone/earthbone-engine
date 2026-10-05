// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for authored effect-material state sent to Godot.

use soe_godot_host::{
    mesh::{EffectMaterial, MaterialDesc},
    nif_effect,
};
use soe_nif::{
    blocks::{flags1, flags2, BlendFactor, TestFunc},
    mesh::{AlphaTest, Blend, EffectParams},
};

fn desc() -> (MaterialDesc, EffectMaterial) {
    (
        MaterialDesc::default(),
        EffectMaterial {
            params: EffectParams::default(),
            blend: None,
            alpha_test: None,
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
        },
    )
}

#[test]
fn rnd_22_depth_write_and_test_follow_authored_flags_and_blending() {
    let (mut material, mut effect) = desc();
    material.shader_flags_1 = flags1::ZBUFFER_TEST;
    material.shader_flags_2 = flags2::ZBUFFER_WRITE;
    let opaque = nif_effect::shader(&material, &effect);
    assert!(opaque.contains("depth_draw_opaque"));
    assert!(!opaque.contains("depth_test_disabled"));
    effect.blend = Some(Blend {
        src: BlendFactor::SrcAlpha,
        dst: BlendFactor::OneMinusSrcAlpha,
    });
    let blended = nif_effect::shader(&material, &effect);
    assert!(blended.contains("depth_draw_never"));
    material.shader_flags_1 = 0;
    material.shader_flags_2 = 0;
    material.double_sided = true;
    let no_depth = nif_effect::shader(&material, &effect);
    assert!(no_depth.contains("cull_disabled"));
    assert!(no_depth.contains("depth_test_disabled"));
    assert!(no_depth.contains("depth_draw_never"));
}

#[test]
fn rnd_22_falloff_palette_vertex_and_soft_flags_are_independent() {
    let (mut material, mut effect) = desc();
    material.shader_flags_1 =
        flags1::USE_FALLOFF | flags1::SOFT_EFFECT | flags1::GREYSCALE_TO_PALETTE_COLOR;
    material.shader_flags_2 = flags2::VERTEX_COLORS;
    let code = nif_effect::shader(&material, &effect);
    assert!(code.contains("if (true)"));
    assert!(code.contains("angle_opacity"));
    assert!(code.contains("texture(palette_texture, vec2(texel.g, tint.r))"));
    assert!(code.contains("COLOR.rgb"));
    assert!(code.contains("if (soft_depth > 0.0)"));
    material.shader_flags_1 |= flags1::VERTEX_ALPHA;
    effect.alpha_test = Some(AlphaTest {
        func: TestFunc::Greater,
        threshold: 0.5,
    });
    let alpha = nif_effect::shader(&material, &effect);
    assert!(alpha.contains("vec4(COLOR.rgb, COLOR.a)"));
    assert!(alpha.contains("opacity > alpha_threshold"));
}

#[test]
fn rnd_22_sampler_dependencies_follow_soft_and_blend_flags() {
    let (mut material, mut effect) = desc();
    material.shader_flags_1 = flags1::ZBUFFER_TEST;
    material.shader_flags_2 = flags2::ZBUFFER_WRITE;
    let opaque = nif_effect::shader(&material, &effect);
    assert!(!opaque.contains("hint_screen_texture"));
    assert!(!opaque.contains("hint_depth_texture"));
    material.shader_flags_1 |= flags1::SOFT_EFFECT;
    let soft = nif_effect::shader(&material, &effect);
    assert!(soft.contains("hint_depth_texture"));
    assert!(!soft.contains("hint_screen_texture"));
    material.shader_flags_1 &= !flags1::SOFT_EFFECT;
    effect.blend = Some(Blend {
        src: BlendFactor::SrcColor,
        dst: BlendFactor::OneMinusSrcColor,
    });
    let fallback = nif_effect::shader(&material, &effect);
    assert!(fallback.contains("hint_screen_texture"));
    assert!(!fallback.contains("hint_depth_texture"));
}

#[test]
fn rnd_22_depth_disabled_opaque_effects_bypass_the_host_depth_prepass() {
    let (mut material, effect) = desc();
    material.shader_flags_2 = flags2::ZBUFFER_WRITE;
    let writes_depth = nif_effect::shader(&material, &effect);
    assert!(!writes_depth.contains("ALPHA ="));
    material.shader_flags_2 = 0;
    let no_depth = nif_effect::shader(&material, &effect);
    assert!(no_depth.contains("depth_draw_never"));
    assert!(no_depth.contains("ALPHA = 1.0;"));
}

#[test]
fn rnd_22_each_ni_alpha_blend_factor_is_translated_without_aliasing() {
    let (material, mut effect) = desc();
    let factors = [
        BlendFactor::One,
        BlendFactor::Zero,
        BlendFactor::SrcColor,
        BlendFactor::OneMinusSrcColor,
        BlendFactor::DstColor,
        BlendFactor::OneMinusDstColor,
        BlendFactor::SrcAlpha,
        BlendFactor::OneMinusSrcAlpha,
        BlendFactor::DstAlpha,
        BlendFactor::OneMinusDstAlpha,
        BlendFactor::SrcAlphaSaturate,
    ];
    for src in factors {
        for dst in factors {
            effect.blend = Some(Blend { src, dst });
            let code = nif_effect::shader(&material, &effect);
            assert!(code.contains("depth_draw_never"), "{src:?}, {dst:?}");
            assert!(code.contains("blend_"), "{src:?}, {dst:?}");
            if !matches!(
                (src, dst),
                (
                    BlendFactor::SrcAlpha | BlendFactor::One,
                    BlendFactor::OneMinusSrcAlpha | BlendFactor::One
                ) | (BlendFactor::DstColor | BlendFactor::One, BlendFactor::Zero)
                    | (BlendFactor::Zero, BlendFactor::SrcColor)
            ) {
                assert!(
                    code.contains("texture(screen_texture, SCREEN_UV)"),
                    "{src:?}, {dst:?}"
                );
            }
        }
    }
}
