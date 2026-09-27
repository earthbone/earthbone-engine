// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! EDT-25 checks generated object LOD geometry and its reported residency.

use std::path::PathBuf;

use soe_godot_host::data_source::DataSource;
use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel};

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|error| panic!("DataSource::open: {error}")))
}

#[test]
fn edt_25_installed_bto_geometry_and_residency_survey() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping installed BTO survey");
        return;
    };
    let (world, editor_id) = source
        .resolve_world("Tamriel")
        .expect("vanilla Tamriel worldspace");
    let mut loaded = None;
    'scan: for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        let span = level.cells();
        for y in (-64..=64).step_by(span as usize) {
            for x in (-64..=64).step_by(span as usize) {
                let block = LodBlockCoord { level, x, y };
                if let Some(result) = source
                    .load_object_lod_block(world, &editor_id, block)
                    .unwrap_or_else(|error| panic!("BTO consumer {block:?}: {error}"))
                {
                    loaded = Some((block, result));
                    break 'scan;
                }
            }
        }
    }
    let (block, loaded) = loaded.expect("vanilla Tamriel ships a BTO object LOD mesh");
    assert!(!loaded.surfaces.is_empty());
    assert!(loaded.triangles > 0);
    assert!(loaded.approx_bytes > 0);
    assert_eq!(
        loaded.triangles,
        loaded
            .surfaces
            .iter()
            .map(|surface| surface.arrays.indices.len() / 3)
            .sum::<usize>()
    );
    assert!(loaded
        .surfaces
        .iter()
        .all(|surface| surface.reference == world));
    assert_eq!(loaded.source_triangles, loaded.triangles);
    assert_eq!(loaded.source_approx_bytes, loaded.approx_bytes);
    eprintln!(
        "EDT-25 {block:?}: {} BTO surfaces, {} triangles, {} resident bytes",
        loaded.surfaces.len(),
        loaded.triangles,
        loaded.approx_bytes
    );
}

#[test]
fn edt_25_object_lod_level_sizes_match_bto_consumer_levels() {
    for (level, cells) in [
        (LodLevel::L4, 4),
        (LodLevel::L8, 8),
        (LodLevel::L16, 16),
        (LodLevel::L32, 32),
    ] {
        let block = LodBlockCoord::containing_from(
            CellCoord::new(-1, cells - 1),
            level,
            CellCoord::new(0, 0),
        );
        assert_eq!(block.level.cells(), cells);
        assert_eq!(block.x, -cells);
        assert_eq!(block.y, 0);
    }
}
