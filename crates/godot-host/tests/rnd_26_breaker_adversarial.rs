// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed BTR materials retain feature 18 textures and usable normals.

use std::path::PathBuf;

use soe_godot_host::data_source::DataSource;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel};

#[test]
fn rnd_26_installed_landscape_lod_has_diffuse_normal_noise_and_finite_normals() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        return;
    };
    let source = DataSource::open(&data).expect("installed data opens");
    let (world, _) = source.resolve_world("Tamriel").expect("Tamriel resolves");
    let mut landscape_surfaces = 0;

    for (x, y) in [(0, -8), (4, -8), (8, -8), (0, -4), (4, -4)] {
        let block = LodBlockCoord {
            level: LodLevel::L4,
            x,
            y,
        };
        let loaded = source
            .load_lod_block(world, "Tamriel", block)
            .expect("BTR loads")
            .expect("BTR is present");
        for surface in &loaded.surfaces {
            let material = &surface.arrays.material;
            if !material.landscape_lod_normals() {
                assert!(!material.landscape_lod_noise, "non-landscape BTR {block:?}");
                continue;
            }
            landscape_surfaces += 1;
            assert!(
                surface.textures.diffuse.is_some(),
                "missing diffuse {block:?}"
            );
            assert!(
                surface.textures.normal.is_some(),
                "missing normal {block:?}"
            );
            assert!(
                surface.textures.terrain_noise.is_some(),
                "missing feature 18 noise {block:?}"
            );
            assert!(
                material.landscape_lod_noise,
                "feature 18 disabled {block:?}"
            );
            assert_eq!(surface.arrays.normals.len(), surface.arrays.positions.len());
            assert!(surface
                .arrays
                .normals
                .iter()
                .flatten()
                .all(|n| n.is_finite()));
        }
    }
    assert!(
        landscape_surfaces > 0,
        "survey had no landscape LOD materials"
    );
}
