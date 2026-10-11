// SPDX-License-Identifier: GPL-3.0-or-later

//! RND-33 installed hearth material survey.

use super::apply_fire_palette_fallback;
use super::{model_for, DataSource};
use crate::mesh::EffectMaterial;
use soe_esm::records::refr::Refr;
use soe_nif::blocks::{flags1, BlendFactor};
use std::path::Path;

#[test]
fn rnd_33_unpaletted_additive_fire_uses_a_sibling_palette() {
    let mut material = crate::mesh::MaterialDesc {
        effect: Some(EffectMaterial {
            params: soe_nif::mesh::EffectParams::default(),
            blend: Some(soe_nif::mesh::Blend {
                src: BlendFactor::SrcAlpha,
                dst: BlendFactor::One,
            }),
            alpha_test: None,
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
            controllers: Vec::new(),
        }),
        ..Default::default()
    };
    apply_fire_palette_fallback(&mut material, Some("textures\\effects\\GradFire.dds"));
    assert_ne!(
        material.shader_flags_1 & flags1::GREYSCALE_TO_PALETTE_COLOR,
        0
    );
    assert_eq!(
        material.effect.as_ref().unwrap().params.greyscale_texture,
        "textures\\effects\\GradFire.dds"
    );
}

#[test]
fn rnd_33_installed_bannered_mare_hearth_uses_additive_palette_effects() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping RND-33 installed hearth survey");
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 2).unwrap();
    let cell = source
        .resolve_cell("WhiterunBanneredMare")
        .expect("installed Bannered Mare cell");
    let loaded = source.stream_cell_id(cell).unwrap();
    let mut found = Vec::new();
    for surface in &loaded.surfaces {
        let Some(effect) = surface.arrays.material.effect.as_ref() else {
            continue;
        };
        let owner = source.order.winner_ref(surface.reference).unwrap();
        let record = source.order.record(owner);
        let refr = Refr::from_record(record).unwrap();
        let base_id = source.order.resolve(owner.file, refr.base);
        let Some(base) = source.order.winner(base_id) else {
            continue;
        };
        let model = model_for(base).unwrap_or_default();
        if !model.to_ascii_lowercase().contains("fx")
            && !base
                .editor_id()
                .is_some_and(|name| name.to_ascii_lowercase().contains("fire"))
        {
            continue;
        }
        found.push((
            surface.reference,
            base.editor_id(),
            model,
            surface.arrays.material.shader_flags_1,
            surface.arrays.material.shader_flags_2,
            effect.blend,
            effect.params.greyscale_texture.clone(),
            surface.textures.greyscale.is_some(),
            effect.params.falloff_start_opacity,
            effect.params.falloff_stop_opacity,
            effect.params.source_texture.clone(),
        ));
    }
    found.sort_by_key(|surface| surface.0);
    println!("RND-33 hearth effect surfaces: {found:#?}");
    let fireplace: Vec<_> = found
        .iter()
        .filter(|surface| {
            surface
                .2
                .to_ascii_lowercase()
                .contains("fireplacewood01burning")
        })
        .collect();
    let campfire: Vec<_> = found
        .iter()
        .filter(|surface| surface.2.to_ascii_lowercase().contains("campfire01burning"))
        .collect();
    assert!(
        !fireplace.is_empty(),
        "Bannered Mare fireplace mesh is present"
    );
    assert!(
        fireplace.iter().any(|surface| {
            surface.5.is_some_and(|blend| {
                blend.src == BlendFactor::SrcAlpha && blend.dst == BlendFactor::One
            })
        }),
        "Bannered Mare fireplace has an additive effect surface"
    );
    assert!(
        fireplace
            .iter()
            .any(|surface| { surface.3 & flags1::GREYSCALE_TO_PALETTE_COLOR != 0 && surface.7 }),
        "Bannered Mare fireplace has an uploaded greyscale palette effect"
    );
    assert!(
        fireplace
            .iter()
            .any(|surface| surface.3 & flags1::USE_FALLOFF != 0),
        "Bannered Mare fireplace has a falloff effect surface"
    );
    assert!(
        campfire.iter().any(|surface| {
            surface.5.is_some_and(|blend| {
                blend.src == BlendFactor::SrcAlpha && blend.dst == BlendFactor::One
            }) && surface.3 & flags1::GREYSCALE_TO_PALETTE_COLOR != 0
                && surface.7
        }),
        "Bannered Mare campfire additive layer inherits a decoded sibling palette"
    );
}

#[test]
fn rnd_33_additive_palette_shader_retains_alpha_depth_and_falloff() {
    let desc = crate::mesh::MaterialDesc {
        shader_flags_1: flags1::USE_FALLOFF
            | flags1::GREYSCALE_TO_PALETTE_COLOR
            | flags1::ZBUFFER_TEST,
        shader_flags_2: 0,
        ..Default::default()
    };
    let effect = EffectMaterial {
        params: soe_nif::mesh::EffectParams::default(),
        blend: Some(soe_nif::mesh::Blend {
            src: BlendFactor::SrcAlpha,
            dst: BlendFactor::One,
        }),
        alpha_test: None,
        uv_offset: [0.0; 2],
        uv_scale: [1.0; 2],
        controllers: Vec::new(),
    };
    let code = crate::nif_effect::shader(&desc, &effect);
    assert!(code.contains("blend_add"));
    assert!(code.contains("depth_draw_never"));
    assert!(code.contains("ALPHA = opacity;"));
    assert!(code.contains("texture(palette_texture, vec2(texel.g, tint.r))"));
    assert!(code.contains("float opacity = tint.a * angle_opacity * (false ? 1.0 : texel.a)"));
    assert!(code.contains("angle_opacity = mix("));
}
