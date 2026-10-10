// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for additive fire effect materials.

use std::path::Path;

use soe_esm::{records::refr::Refr, Tag};
use soe_godot_host::{
    data_source::DataSource,
    mesh::{EffectMaterial, MaterialDesc},
    nif_effect,
};
use soe_nif::{
    blocks::{flags1, BlendFactor},
    mesh::{Blend, EffectParams},
};

fn effect(blend: (BlendFactor, BlendFactor)) -> EffectMaterial {
    EffectMaterial {
        params: EffectParams::default(),
        blend: Some(Blend {
            src: blend.0,
            dst: blend.1,
        }),
        alpha_test: None,
        uv_offset: [0.0; 2],
        uv_scale: [1.0; 2],
    }
}

#[test]
fn rnd_33_additive_src_alpha_uses_opacity_without_hull_fill_or_depth_write() {
    let mut desc = MaterialDesc {
        shader_flags_1: flags1::ZBUFFER_TEST | flags1::USE_FALLOFF,
        ..Default::default()
    };
    let effect = effect((BlendFactor::SrcAlpha, BlendFactor::One));
    let code = nif_effect::shader(&desc, &effect);
    assert!(code.contains("blend_add"));
    assert!(code.contains("depth_draw_never"));
    assert!(code.contains("ALPHA = opacity;"));
    assert!(!code.contains("ALPHA = 1.0;"));
    assert!(!code.contains("ALBEDO *= opacity"));
    assert!(!code.contains("EMISSION *= opacity"));
    assert!(code.contains("float opacity = tint.a * angle_opacity * (false ? 1.0 : texel.a)"));
    assert!(code.contains("angle_opacity = mix("));

    desc.shader_flags_1 &= !flags1::USE_FALLOFF;
    let without_falloff = nif_effect::shader(&desc, &effect);
    assert!(without_falloff.contains("ALPHA = opacity;"));
}

#[test]
fn rnd_33_palette_color_and_alpha_are_independent_source_lookups() {
    let mut desc = MaterialDesc {
        shader_flags_1: flags1::GREYSCALE_TO_PALETTE_COLOR,
        ..Default::default()
    };
    let mut effect = effect((BlendFactor::SrcAlpha, BlendFactor::One));
    let color = nif_effect::shader(&desc, &effect);
    assert!(color.contains("texture(palette_texture, vec2(texel.g, tint.r)).rgb"));
    assert!(color.contains("ALPHA = opacity;"));
    assert!(color.contains("if (false) { opacity = texture(palette_texture"));

    desc.shader_flags_1 |= flags1::GREYSCALE_TO_PALETTE_ALPHA;
    let both = nif_effect::shader(&desc, &effect);
    assert!(both.contains("texture(palette_texture, vec2(texel.g, tint.r)).rgb"));
    assert!(both
        .contains("if (true) { opacity = texture(palette_texture, vec2(texel.a, opacity)).a; }"));
    assert!(both.contains("ALPHA = opacity;"));

    effect.blend = Some(Blend {
        src: BlendFactor::One,
        dst: BlendFactor::One,
    });
    let source_one = nif_effect::shader(&desc, &effect);
    assert!(source_one.contains("ALPHA = 1.0;"));
}

#[test]
fn rnd_33_installed_bannered_mare_additive_fire_layers_resolve_palette_textures() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping RND-33 installed fire survey");
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 4).unwrap();
    let cell = source.resolve_cell("WhiterunBanneredMare").unwrap();
    let loaded = source.stream_cell_id(cell).unwrap();
    let order = source.shared_order();
    let mut fireplace_layers = Vec::new();

    for surface in &loaded.surfaces {
        let Some(effect) = surface.arrays.material.effect.as_ref() else {
            continue;
        };
        let Some(blend) = effect.blend else {
            continue;
        };
        if blend.src != BlendFactor::SrcAlpha || blend.dst != BlendFactor::One {
            continue;
        }
        let Some(owner) = order.winner_ref(surface.reference) else {
            continue;
        };
        let reference = order.record(owner);
        let Ok(refr) = Refr::from_record(reference) else {
            continue;
        };
        let base_id = order.resolve(owner.file, refr.base);
        let Some(base) = order.winner(base_id) else {
            continue;
        };
        let model = base
            .find(Tag::new(b"MODL"))
            .map(soe_esm::cursor::zstring)
            .unwrap_or_default();
        if !model
            .to_ascii_lowercase()
            .contains("fireplacewood01burning")
        {
            continue;
        }
        fireplace_layers.push(surface);
        assert_ne!(
            surface.arrays.material.shader_flags_1 & flags1::GREYSCALE_TO_PALETTE_COLOR,
            0,
            "additive fireplace layer must sample its greyscale color palette"
        );
        assert!(
            !effect.params.greyscale_texture.is_empty(),
            "additive fireplace layer must have a palette path"
        );
        assert!(
            surface.textures.greyscale.is_some(),
            "additive fireplace palette must decode from installed data"
        );
        let shader = nif_effect::shader(&surface.arrays.material, effect);
        assert!(shader.contains("blend_add"));
        assert!(shader.contains("depth_draw_never"));
        assert!(shader.contains("ALPHA = opacity;"));
    }

    assert!(
        !fireplace_layers.is_empty(),
        "placed Bannered Mare fireplace must expose additive fire layers"
    );
}
