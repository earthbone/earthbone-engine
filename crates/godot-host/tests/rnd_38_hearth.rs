// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Hearth layer audit through the installed VFS and production cell builder.

use soe_esm::{records::refr::Refr, Tag};
use soe_godot_host::data_source::DataSource;
use std::path::Path;

#[test]
fn rnd_38_installed_hearth_audits_every_effect_layer() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 2).unwrap();
    let loaded = source
        .stream_cell_id(source.resolve_cell("WhiterunBanneredMare").unwrap())
        .unwrap();
    let order = source.shared_order();
    let mut hearths = std::collections::BTreeMap::new();
    for surface in &loaded.surfaces {
        let Some(owner) = order.winner_ref(surface.reference) else {
            continue;
        };
        let Ok(refr) = Refr::from_record(order.record(owner)) else {
            continue;
        };
        let Some(base) = order.winner(order.resolve(owner.file, refr.base)) else {
            continue;
        };
        let model = base
            .find(Tag::new(b"MODL"))
            .map(soe_esm::cursor::zstring)
            .unwrap_or_default();
        if ![
            "clutter/woodfires/fireplacewood01burning.nif",
            "clutter/woodfires/campfire01burning.nif",
        ]
        .contains(&soe_bsa::normalize(&model).as_str())
        {
            continue;
        }
        let counts = hearths
            .entry(surface.reference)
            .or_insert((model.clone(), [0_usize; 4]));
        let desc = &surface.arrays.material;
        if desc
            .diffuse
            .as_deref()
            .is_some_and(|path| path.to_ascii_lowercase().contains("vaportilenormal"))
        {
            counts.1[1] += 1;
            assert!(
                desc.refraction.is_some(),
                "{model}: normal-map hull takes the distortion path"
            );
            assert!(surface.textures.diffuse.is_some());
        }
        if let Some(effect) = &desc.effect {
            counts.1[0] += 1;
            assert_ne!(
                desc.shader_flags_1 & soe_nif::blocks::flags1::GREYSCALE_TO_PALETTE_COLOR,
                0
            );
            counts.1[2] +=
                usize::from(desc.shader_flags_1 & soe_nif::blocks::flags1::USE_FALLOFF != 0);
            counts.1[3] += usize::from(
                effect
                    .blend
                    .is_some_and(|blend| blend.dst == soe_nif::blocks::BlendFactor::One),
            );
            assert!(surface.textures.diffuse.is_some());
            if !effect.params.greyscale_texture.is_empty() {
                assert!(
                    surface.textures.greyscale.is_some(),
                    "{model}: authored palette"
                );
            }
        }
    }
    assert!(hearths.len() >= 2, "both burning models are placed");
    let mut models = std::collections::BTreeSet::new();
    for (_, (model, counts)) in hearths {
        let fireplace = model.to_ascii_lowercase().contains("fireplace");
        assert_eq!(
            counts,
            if fireplace {
                [3, 1, 2, 1]
            } else {
                [3, 1, 0, 3]
            },
            "{model}: effects, distortion, falloff, additive"
        );
        models.insert(fireplace);
    }
    assert_eq!(models.len(), 2);
}

#[test]
fn rnd_38_palette_uses_green_and_alpha_coordinates_with_additive_falloff() {
    use soe_godot_host::mesh::{EffectMaterial, MaterialDesc};
    use soe_nif::blocks::{flags1, BlendFactor};
    let material = MaterialDesc {
        shader_flags_1: flags1::GREYSCALE_TO_PALETTE_COLOR
            | flags1::GREYSCALE_TO_PALETTE_ALPHA
            | flags1::USE_FALLOFF,
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
    let shader = soe_godot_host::nif_effect::shader(&material, &effect);
    assert!(shader.contains("blend_add"));
    assert!(shader.contains("ALPHA = opacity;"));
    assert!(shader.contains("depth_draw_never"));
    assert!(shader.contains("texture(palette_texture, vec2(texel.g, tint.r))"));
    assert!(shader.contains("texture(palette_texture, vec2(texel.a, opacity)).a"));
    assert!(shader.contains("if (true) {\n        vec3 position"));
    assert!(shader.contains("angle_opacity = mix("));
}
