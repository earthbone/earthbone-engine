// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-10 check for BTO object LOD paths, distance separation and streamed
//! worldspace/block identity. The installed-data case reads a real BTO when
//! `SKYRIM_DATA` is available.

use std::collections::HashSet;
use std::path::PathBuf;

use soe_esm::GlobalFormId;
use soe_godot_host::data_source::DataSource;
use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{
    bto_path, LodBlockCoord, LodLevel, TerrainLodConfig, TerrainLodStreamer,
};

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}")))
}

#[test]
fn rnd_10_bto_path_uses_world_level_and_south_west_cell() {
    let path = bto_path(
        "Tamriel",
        LodBlockCoord {
            level: LodLevel::L32,
            x: -32,
            y: 64,
        },
    );
    assert_eq!(path, "meshes/terrain/Tamriel/Objects/tamriel.32.-32.64.bto");
}

#[test]
fn rnd_10_object_lod_stays_outside_near_geometry_and_keeps_block_keys() {
    let config = TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 2,
        lod_radius: 10,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
        budget_bytes: 0,
    };
    let mut streamer = TerrainLodStreamer::new(config).expect("valid object LOD config");
    let focus = CellCoord::new(0, 0);
    let plan = streamer.settle(focus);
    assert!(
        !plan.load.is_empty(),
        "distant object LOD should be planned"
    );

    let mut identities = HashSet::new();
    for block in plan.load {
        let distance = block.ring_distance(focus);
        assert!(
            distance > config.full_res_radius,
            "BTO block {block:?} overlaps nearby full resolution geometry at distance {distance}"
        );
        assert!(distance <= config.lod_radius);
        assert!(identities.insert(("Tamriel", block)));
    }
    assert_eq!(identities.len(), streamer.resident_count());
    assert!(streamer
        .resident_blocks()
        .all(|block| { block.ring_distance(focus) > config.full_res_radius }));
}

#[test]
fn rnd_10_installed_bto_builds_geometry_with_world_identity() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the installed BTO object LOD read");
        return;
    };
    let world = GlobalFormId {
        file: 0,
        object: 0x3c,
    };
    let mut found = None;
    'scan: for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        let span = level.cells();
        for y in (-64..=64).step_by(span as usize) {
            for x in (-64..=64).step_by(span as usize) {
                let block = LodBlockCoord { level, x, y };
                let loaded = source
                    .load_object_lod_block(world, "Tamriel", block)
                    .unwrap_or_else(|e| panic!("load_object_lod_block {block:?}: {e}"));
                if let Some(loaded) = loaded {
                    found = Some((block, loaded));
                    break 'scan;
                }
            }
        }
    }
    let (block, loaded) = found.expect("vanilla Tamriel ships object LOD BTO geometry");
    assert!(!loaded.surfaces.is_empty());
    assert!(loaded.approx_bytes > 0);
    assert!(loaded
        .surfaces
        .iter()
        .all(|surface| surface.reference == world));
    eprintln!(
        "read Tamriel BTO block {block:?}: {} surfaces, {} estimated bytes",
        loaded.surfaces.len(),
        loaded.approx_bytes
    );
}
