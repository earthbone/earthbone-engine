// SPDX-License-Identifier: GPL-3.0-or-later

//! Independent NIF and DDS fixtures exercise effect shading through the host.

use super::{fixture, DataSource};
use crate::{batch::BatchKey, mesh::MaterialKey};
use soe_esm::{FileHeader, Plugin};
use soe_nif::{
    blocks::{flags1, flags2},
    vertex::attr,
};
use std::path::Path;

fn texture(color: [u8; 4]) -> Vec<u8> {
    use soe_dds::header::{caps, flags, pf, Header, PixelFormat};
    let mut bytes = Header {
        flags: flags::CAPS | flags::HEIGHT | flags::WIDTH | flags::PIXELFORMAT | flags::PITCH,
        height: 8,
        width: 8,
        pitch_or_linear_size: 32,
        depth: 0,
        mip_map_count: 1,
        pixel_format: PixelFormat {
            flags: pf::RGB | pf::ALPHAPIXELS,
            four_cc: [0; 4],
            rgb_bit_count: 32,
            r_mask: 0xff,
            g_mask: 0xff00,
            b_mask: 0xff0000,
            a_mask: 0xff000000,
        },
        caps: caps::TEXTURE,
        caps2: 0,
        dx10: None,
    }
    .to_bytes();
    bytes.extend(color.repeat(64));
    bytes
}

fn model(index: usize) -> Vec<u8> {
    model_with_blend(index, None)
}

fn model_with_blend(index: usize, blend: Option<(u16, u16)>) -> Vec<u8> {
    let mut writer = fixture::NifWriter::new(100);
    let f1 = flags1::ZBUFFER_TEST
        | match index {
            5 => flags1::USE_FALLOFF,
            6 => flags1::VERTEX_ALPHA,
            7 => flags1::GREYSCALE_TO_PALETTE_COLOR | flags1::GREYSCALE_TO_PALETTE_ALPHA,
            8 => flags1::SOFT_EFFECT,
            12 => flags1::GREYSCALE_TO_PALETTE_COLOR,
            13 => flags1::GREYSCALE_TO_PALETTE_ALPHA,
            _ => 0,
        };
    let f2 = flags2::DOUBLE_SIDED
        | flags2::ZBUFFER_WRITE
        | if index == 6 { flags2::VERTEX_COLORS } else { 0 };
    let mut bytes = writer.effect_shader(
        f1,
        f2,
        if matches!(index, 7 | 12 | 13) {
            "textures/palette-source.dds"
        } else {
            "textures/source.dds"
        },
        [0.4, 0.4, 0.4, 0.25],
    );
    // CONTEXT: Replace the writer's final fixed effect fields, not header offsets.
    let palette_len = "textures\\effects\\grey.dds".len();
    let soft_offset = bytes.len() - palette_len - 4 - 4;
    bytes[soft_offset - 4..soft_offset].copy_from_slice(&1.0_f32.to_le_bytes());
    if index == 5 {
        let falloff = soft_offset - 4 - 16 - 16;
        for (slot, value) in [0.0_f32, 1.0, 0.1, 0.5].into_iter().enumerate() {
            bytes[falloff + slot * 4..falloff + slot * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    let shader = writer.add("BSEffectShaderProperty", bytes);
    let (src, dst) = blend.unwrap_or(match index {
        1 => (6, 0),
        2 => (0, 0),
        3 => (0, 7),
        4 => (4, 1),
        _ => (6, 7),
    });
    let alpha_flags =
        1 | (src << 1) | (dst << 5) | if index == 11 { (1 << 9) | (4 << 10) } else { 0 };
    let bytes = writer.alpha_property(alpha_flags, 128);
    let alpha = writer.add("NiAlphaProperty", bytes);
    let (mut vertices, triangles) = fixture::quad();
    for vertex in &mut vertices {
        let [x, y, _] = vertex.position;
        vertex.position = [(x - 0.5) * 70.0, 0.0, (y - 0.5) * 70.0];
        vertex.normal = [0.0, -1.0, 0.0];
        vertex.color = [128, 128, 128, 128];
    }
    let bytes = writer.bs_tri_shape(
        "EffectQuad",
        [0.0; 3],
        -1,
        shader,
        if index == 10 { -1 } else { alpha },
        attr::VERTEX | attr::UVS | attr::NORMALS | attr::COLORS,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", bytes);
    writer.roots = vec![shape];
    writer.finish()
}

fn make_fixture(directory: &Path) {
    std::fs::create_dir_all(directory.join("meshes")).unwrap();
    std::fs::create_dir_all(directory.join("textures/effects")).unwrap();
    std::fs::write(directory.join("textures/source.dds"), texture([255; 4])).unwrap();
    std::fs::write(
        directory.join("textures/palette-source.dds"),
        texture([255, 64, 128, 192]),
    )
    .unwrap();
    let mut palette = texture([0; 4]);
    let start = palette.len() - 8 * 8 * 4;
    for y in 0..8 {
        for x in 0..8 {
            let color = if x < 4 && y < 4 {
                [128, 64, 32, 192]
            } else if x >= 4 && y < 4 {
                [0, 128, 0, 128]
            } else {
                [255, 0, 0, 255]
            };
            let offset = start + (y * 8 + x) * 4;
            palette[offset..offset + 4].copy_from_slice(&color);
        }
    }
    std::fs::write(directory.join("textures/effects/grey.dds"), palette).unwrap();
    for i in 0..14 {
        std::fs::write(directory.join(format!("meshes/effect{i}.nif")), model(i)).unwrap();
    }
    for src in 0..=10 {
        for dst in 0..=10 {
            std::fs::write(
                directory.join(format!("meshes/blend{src}_{dst}.nif")),
                model_with_blend(0, Some((src, dst))),
            )
            .unwrap();
        }
    }
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            directory.join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
}

#[test]
fn rnd_22_effect_pipeline_preserves_blend_palette_falloff_and_vertices() {
    let temp = tempfile::tempdir().unwrap();
    let export = std::env::var_os("SOE_RND22_FIXTURE");
    let directory = export.as_deref().map(Path::new).unwrap_or(temp.path());
    make_fixture(directory);
    let source = DataSource::open(directory).unwrap();
    for i in 0..14 {
        let surfaces = source
            .item_preview_surfaces(&format!("effect{i}.nif"))
            .unwrap();
        let (arrays, textures) = &surfaces[0];
        let material = &arrays.material;
        let effect = material.effect.as_ref().unwrap();
        assert_eq!(effect.params.base_color_scale, 1.0);
        assert_eq!(effect.params.base_color, [0.4, 0.4, 0.4, 0.25]);
        assert_eq!(effect.blend.is_some(), i != 10);
        assert_eq!(textures.greyscale.is_some(), matches!(i, 7 | 12 | 13));
        assert!(arrays.colors.iter().all(|c| *c == [128.0 / 255.0; 4]));
        if i == 5 {
            assert_eq!(
                [
                    effect.params.falloff_start_angle,
                    effect.params.falloff_stop_angle,
                    effect.params.falloff_start_opacity,
                    effect.params.falloff_stop_opacity
                ],
                [0.0, 1.0, 0.1, 0.5]
            );
        }
    }
}

#[test]
fn rnd_22_cache_and_batch_keys_separate_every_effect_parameter() {
    let file = soe_nif::parse(&model(0)).unwrap();
    let meshes = file.meshes().unwrap();
    let data = super::mesh_data_from(&meshes[0], &soe_nif::Transform::IDENTITY);
    let arrays = crate::mesh::build_surface(&data, crate::mesh::MeshOptions::default()).unwrap();
    let mut variants = Vec::new();
    for i in 0..16 {
        let mut v = arrays.clone();
        let e = v.material.effect.as_mut().unwrap();
        match i {
            0 => e.params.base_color[0] += 0.1,
            1 => e.params.base_color[3] += 0.1,
            2 => e.params.base_color_scale += 0.1,
            3 => e.params.falloff_start_angle -= 0.1,
            4 => e.params.falloff_stop_angle -= 0.1,
            5 => e.params.falloff_start_opacity -= 0.1,
            6 => e.params.falloff_stop_opacity -= 0.1,
            7 => e.params.soft_falloff_depth += 1.0,
            8 => e.params.greyscale_texture = "other.dds".into(),
            9 => e.params.lighting_influence = 0.5,
            10 => e.params.clamp_mode = 0,
            11 => e.uv_offset[0] = 0.1,
            12 => e.uv_scale[1] = 2.0,
            13 => e.blend.as_mut().unwrap().dst = soe_nif::blocks::BlendFactor::One,
            14 => {
                e.alpha_test = Some(soe_nif::mesh::AlphaTest {
                    func: soe_nif::blocks::TestFunc::Greater,
                    threshold: 0.5,
                });
            }
            _ => v.material.shader_flags_1 |= flags1::SOFT_EFFECT,
        }
        variants.push(v);
    }
    for variant in variants {
        assert_ne!(
            MaterialKey::from(&arrays.material),
            MaterialKey::from(&variant.material)
        );
        assert_ne!(BatchKey::of(&arrays), BatchKey::of(&variant));
    }
}

#[test]
fn rnd_22_installed_mist_and_window_materials_resolve_their_textures() {
    let Some(directory) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&directory)).unwrap();
    let mut near_whiterun = Vec::new();
    let origin = source
        .resolve_exterior_block("WhiterunOrigin", 0)
        .unwrap()
        .origin;
    for (cell, _) in source.cell_index().cells() {
        let Some(grid) = source.cell_index().grid_of(cell) else {
            continue;
        };
        let Some(world) = source.order.winner(grid.world) else {
            continue;
        };
        if world.editor_id().as_deref() != Some("Tamriel")
            || (grid.x - origin.0).abs() > 5
            || (grid.y - origin.1).abs() > 5
        {
            continue;
        }
        let name = format!("{},{}", grid.x, grid.y);
        for id in source.cell_index().references(cell) {
            let Some(reference) = source.order.winner_ref(*id) else {
                continue;
            };
            let record = source.order.record(reference);
            if record.tag != soe_esm::tag::REFR {
                continue;
            }
            let refr = soe_esm::records::refr::Refr::from_record(record).unwrap();
            let base = source.order.resolve(reference.file, refr.base);
            let Some(model) = source.order.winner(base).and_then(|r| {
                super::model_for(r).or_else(|| {
                    r.find(soe_esm::Tag::new(b"MODL"))
                        .map(soe_esm::cursor::zstring)
                })
            }) else {
                continue;
            };
            if ["mist", "fog", "cloud"]
                .iter()
                .any(|word| model.to_lowercase().contains(word))
            {
                near_whiterun.push((name.clone(), model));
            }
        }
    }
    near_whiterun.sort();
    eprintln!("Whiterun mist placements: {near_whiterun:?}");
    assert!(
        near_whiterun.iter().any(|(_, model)| soe_bsa::normalize(model) == "effects/ambient/fxmistlow01halfvis.nif"),
        "captured mist is placed near Whiterun"
    );
    for model in [
        "effects/ambient/fxmistlow01halfvis.nif",
        "architecture/windhelm/effects/whfxwindowglow01.nif",
    ] {
        let surfaces = source.item_preview_surfaces(model).unwrap();
        let effects: Vec<_> = surfaces
            .iter()
            .filter(|(a, _)| {
                a.material
                    .effect
                    .as_ref()
                    .is_some_and(|e| e.blend.is_some())
            })
            .collect();
        assert!(!effects.is_empty(), "{model}");
        for (arrays, textures) in effects {
            let effect = arrays.material.effect.as_ref().unwrap();
            assert!(textures.diffuse.is_some(), "{model}: source texture");
            if arrays.material.shader_flags_1
                & (flags1::GREYSCALE_TO_PALETTE_COLOR | flags1::GREYSCALE_TO_PALETTE_ALPHA)
                != 0
            {
                assert!(
                    textures.greyscale.is_some(),
                    "{model}: palette {}",
                    effect.params.greyscale_texture
                );
            }
            assert!(
                crate::nif_effect::shader(&arrays.material, effect).contains("depth_draw_never")
            );
        }
    }
}

#[test]
fn rnd_22_effect_vertex_color_length_is_validated() {
    let data = crate::mesh::MeshData {
        positions: vec![[0.0; 3]; 3],
        indices: vec![0, 1, 2],
        colors: Some(vec![[1.0; 4]; 2]),
        ..Default::default()
    };
    assert_eq!(
        crate::mesh::build_surface(&data, crate::mesh::MeshOptions::default()).unwrap_err(),
        crate::mesh::MeshError::AttributeLength {
            attribute: "colors",
            got: 2,
            vertices: 3
        }
    );
}
