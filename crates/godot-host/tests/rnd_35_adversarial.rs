// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for real loaded terrain and distant LOD materials.

use soe_godot_host::data_source::{DataSource, TextureData};
use std::path::Path;

fn high_frequency_energy(image: &TextureData) -> f32 {
    let TextureData::Rgba8(image) = image else {
        return 0.0;
    };
    if image.width < 2 || image.height < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    let mut samples = 0;
    for y in 0..image.height as usize {
        for x in 0..image.width as usize {
            let at = (y * image.width as usize + x) * 4;
            for next in [
                (y * image.width as usize + (x + 1) % image.width as usize) * 4,
                (((y + 1) % image.height as usize) * image.width as usize + x) * 4,
            ] {
                for channel in 0..3 {
                    total +=
                        f32::from(image.rgba[at + channel].abs_diff(image.rgba[next + channel]))
                            / 255.0;
                    samples += 1;
                }
            }
        }
    }
    total / samples as f32
}

#[test]
fn rnd_35_whiterun_land_and_btr_lod_keep_authored_texture_detail() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let loaded = source.stream_exterior("WhiterunOrigin", 2).unwrap();
    let mut land = 0;
    for (_, cell) in &loaded.cells {
        for surface in &cell.surfaces {
            if surface
                .arrays
                .material
                .diffuse
                .as_deref()
                .is_some_and(|p| p.starts_with("terrain:"))
            {
                land += 1;
                let texture = surface
                    .textures
                    .diffuse
                    .as_deref()
                    .expect("baked LAND texture");
                assert!(
                    high_frequency_energy(texture) > 0.02,
                    "LAND texture lost local detail"
                );
            }
        }
    }
    assert_eq!(land, 25, "radius two loads all LAND cells");

    let (world, world_editor_id) = source.lod_worldspace(loaded.block.world).unwrap();
    let blocks = source.scenery_lod_blocks(
        &world_editor_id,
        soe_godot_host::terrain_lod::LodKind::Terrain,
    );
    let mut textured_lod = 0;
    for block in blocks {
        let Some(lod) = source
            .load_lod_block(world, &world_editor_id, block)
            .unwrap()
        else {
            continue;
        };
        for surface in lod.surfaces {
            if surface.arrays.material.shader_flags_2 & soe_nif::blocks::flags2::LOD_LANDSCAPE == 0
            {
                continue;
            }
            assert!(
                surface.textures.diffuse.is_some(),
                "landscape LOD diffuse resolves"
            );
            assert!(
                surface.textures.normal.is_some(),
                "landscape LOD normal resolves"
            );
            textured_lod += 1;
        }
        if textured_lod > 0 {
            break;
        }
    }
    assert!(
        textured_lod > 0,
        "Whiterun landscape LOD has textured surfaces"
    );
}
