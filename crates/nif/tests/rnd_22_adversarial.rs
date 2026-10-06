// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial effect-property parsing checks from RND-22.

mod common;

use common::{quad, NifWriter};
use soe_nif::{
    blocks::{flags1, flags2, AlphaProperty, BlendFactor, TestFunc},
    parse,
    vertex::attr,
};

fn effect_mesh(alpha_flags: Option<u16>) -> Vec<u8> {
    let mut writer = NifWriter::new(100);
    let shader = writer.effect_shader(
        flags1::ZBUFFER_TEST,
        flags2::ZBUFFER_WRITE,
        "textures/effect.dds",
        [0.2, 0.3, 0.4, 0.5],
    );
    let shader = writer.add("BSEffectShaderProperty", shader);
    let alpha = if let Some(flags) = alpha_flags {
        let alpha = writer.alpha_property(flags, 128);
        writer.add("NiAlphaProperty", alpha)
    } else {
        -1
    };
    let (vertices, triangles) = quad();
    let shape = writer.bs_tri_shape(
        "Effect",
        [0.0; 3],
        -1,
        shader,
        alpha,
        attr::VERTEX | attr::UVS,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", shape);
    writer.roots = vec![shape];
    writer.finish()
}

#[test]
fn rnd_22_disabled_alpha_flags_do_not_enable_blending_or_testing() {
    let flags = (6 << 1) | (7 << 5) | (4 << 10);
    let file = parse(&effect_mesh(Some(flags))).unwrap();
    let material = &file.meshes().unwrap()[0].material;
    assert_eq!(material.blend, None);
    assert_eq!(material.alpha_test, None);
}

#[test]
fn rnd_22_alpha_property_factor_and_test_codes_follow_nif_values() {
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
    for (code, expected) in factors.into_iter().enumerate() {
        let parsed = AlphaProperty {
            flags: 1 | ((code as u16) << 1) | ((code as u16) << 5),
            ..Default::default()
        };
        assert_eq!(parsed.src_blend(), expected, "source factor {code}");
        assert_eq!(parsed.dst_blend(), expected, "destination factor {code}");
    }
    let tests = [
        TestFunc::Always,
        TestFunc::Less,
        TestFunc::Equal,
        TestFunc::LessEqual,
        TestFunc::Greater,
        TestFunc::NotEqual,
        TestFunc::GreaterEqual,
        TestFunc::Never,
    ];
    for (code, expected) in tests.into_iter().enumerate() {
        let parsed = AlphaProperty {
            flags: (1 << 9) | ((code as u16) << 10),
            ..Default::default()
        };
        assert_eq!(parsed.test_func(), expected, "test function {code}");
    }
}

#[test]
fn rnd_22_missing_alpha_property_keeps_effect_unblended() {
    let file = parse(&effect_mesh(None)).unwrap();
    let material = &file.meshes().unwrap()[0].material;
    assert!(material.effect.is_some());
    assert_eq!(material.blend, None);
    assert_eq!(material.alpha_test, None);
}

#[test]
fn rnd_22_out_of_range_alpha_property_reference_is_an_error() {
    let mut writer = NifWriter::new(100);
    let effect = writer.effect_shader(0, 0, "", [1.0; 4]);
    let shader = writer.add("BSEffectShaderProperty", effect);
    let (vertices, triangles) = quad();
    let shape = writer.bs_tri_shape(
        "Effect",
        [0.0; 3],
        -1,
        shader,
        999,
        attr::VERTEX,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", shape);
    writer.roots = vec![shape];
    let bytes = writer.finish();
    let file = parse(&bytes).unwrap();
    assert!(matches!(file.meshes(), Err(soe_nif::Error::BadRef { .. })));
}
