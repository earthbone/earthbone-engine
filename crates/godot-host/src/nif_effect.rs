// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot effect shaders from nifly Shaders.cpp, OpenMW bs/nolighting shaders
//! and Community Shaders Effect.hlsl palette coordinates. Native Godot blend
//! modes handle ordinary pairs. Other pairs use Godot's opaque screen copy.

use crate::mesh::{EffectMaterial, MaterialDesc};
use soe_nif::blocks::{flags1, flags2, BlendFactor, TestFunc};

/// Build the effect shader with authored depth, culling and blend state.
pub fn shader(desc: &MaterialDesc, effect: &EffectMaterial) -> String {
    let f1 = desc.shader_flags_1;
    let f2 = desc.shader_flags_2;
    let blended = effect.blend.is_some();
    let depth = if blended || f2 & flags2::ZBUFFER_WRITE == 0 {
        "depth_draw_never"
    } else {
        "depth_draw_opaque"
    };
    let cull = if desc.double_sided {
        "cull_disabled"
    } else {
        "cull_back"
    };
    let test = if f1 & flags1::ZBUFFER_TEST == 0 {
        ", depth_test_disabled"
    } else {
        ""
    };
    let (blend, output) = blend_output(effect);
    let alpha_test = effect
        .alpha_test
        .map(|a| {
            let comparison = match a.func {
                TestFunc::Always => "true",
                TestFunc::Never => "false",
                TestFunc::Less => "opacity < alpha_threshold",
                TestFunc::Equal => "opacity == alpha_threshold",
                TestFunc::LessEqual => "opacity <= alpha_threshold",
                TestFunc::Greater => "opacity > alpha_threshold",
                TestFunc::NotEqual => "opacity != alpha_threshold",
                TestFunc::GreaterEqual => "opacity >= alpha_threshold",
            };
            format!("if (!({comparison})) {{ discard; }}")
        })
        .unwrap_or_default();
    let palette_color = f1 & flags1::GREYSCALE_TO_PALETTE_COLOR != 0;
    let palette_alpha = f1 & flags1::GREYSCALE_TO_PALETTE_ALPHA != 0;
    let source_hint = if palette_color || palette_alpha {
        ""
    } else {
        "source_color, "
    };
    let template = include_str!("nif_effect.gdshader");
    template
        .replace("$MODES", &format!("{depth}, {cull}, {blend}{test}"))
        .replace("$SOURCE_HINT", source_hint)
        .replace(
            "$FALLOFF",
            if f1 & flags1::USE_FALLOFF != 0 {
                "true"
            } else {
                "false"
            },
        )
        .replace(
            "$VERTEX_COLOR",
            if f2 & flags2::VERTEX_COLORS != 0 {
                "COLOR.rgb"
            } else {
                "vec3(1.0)"
            },
        )
        .replace(
            "$VERTEX_ALPHA",
            if f1 & flags1::VERTEX_ALPHA != 0 {
                "COLOR.a"
            } else {
                "1.0"
            },
        )
        .replace(
            "$PALETTE_COLOR",
            if palette_color { "true" } else { "false" },
        )
        .replace(
            "$PALETTE_ALPHA",
            if palette_alpha { "true" } else { "false" },
        )
        .replace(
            "$SOFT",
            if f1 & flags1::SOFT_EFFECT != 0 {
                "true"
            } else {
                "false"
            },
        )
        .replace("$ALPHA_TEST", &alpha_test)
        .replace("$OUTPUT", &output)
}

fn blend_output(effect: &EffectMaterial) -> (&'static str, String) {
    use BlendFactor::{DstColor, One, OneMinusSrcAlpha, SrcAlpha, SrcColor, Zero};
    match effect.blend.map(|b| (b.src, b.dst)) {
        None => ("blend_mix", String::new()),
        Some((SrcAlpha, OneMinusSrcAlpha)) => ("blend_mix", "ALPHA = opacity;".into()),
        Some((One, OneMinusSrcAlpha)) => ("blend_premul_alpha", "ALPHA = opacity;".into()),
        Some((SrcAlpha, One)) => (
            "blend_add",
            "ALBEDO *= opacity; EMISSION *= opacity; ALPHA = 1.0;".into(),
        ),
        Some((One, One)) => ("blend_add", "ALPHA = 1.0;".into()),
        Some((DstColor, Zero) | (Zero, SrcColor)) => ("blend_mul", "ALPHA = 1.0;".into()),
        Some((One, Zero)) => ("blend_mix", "ALPHA = 1.0;".into()),
        Some((src, dst)) => (
            "blend_mix",
            format!(
                "vec4 source = vec4(radiance, opacity);
    vec4 destination = texture(screen_texture, SCREEN_UV);
    ALBEDO = vec3(0.0);
    EMISSION = (source * ({}) + destination * ({})).rgb;
    ALPHA = 1.0;",
                factor(src),
                factor(dst)
            ),
        ),
    }
}

fn factor(factor: BlendFactor) -> &'static str {
    use BlendFactor::*;
    match factor {
        One => "vec4(1.0)",
        Zero => "vec4(0.0)",
        SrcColor => "source",
        OneMinusSrcColor => "vec4(1.0) - source",
        DstColor => "destination",
        OneMinusDstColor => "vec4(1.0) - destination",
        SrcAlpha => "vec4(source.a)",
        OneMinusSrcAlpha => "vec4(1.0 - source.a)",
        DstAlpha => "vec4(destination.a)",
        OneMinusDstAlpha => "vec4(1.0 - destination.a)",
        SrcAlphaSaturate => "vec4(vec3(min(source.a, 1.0 - destination.a)), 1.0)",
    }
}
