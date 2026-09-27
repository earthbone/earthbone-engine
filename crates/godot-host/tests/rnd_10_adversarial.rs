// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial RND-10 checks for BTO block identity and distance boundaries.

use soe_godot_host::streaming::CellCoord;
use soe_godot_host::data_source::DataSource;
use soe_godot_host::terrain_lod::{
    bto_path, LodBlockCoord, LodLevel, TerrainLodConfig, TerrainLodStreamer,
};
use soe_esm::GlobalFormId;
use std::path::PathBuf;

#[test]
fn rnd_10_negative_cells_align_bto_name_to_the_world_anchor() {
    let anchor = CellCoord::new(-3, 5);
    let block = LodBlockCoord::containing_from(CellCoord::new(-4, 4), LodLevel::L4, anchor);
    assert_eq!((block.x, block.y), (-7, 1));
    assert_eq!(
        bto_path("Tamriel", block),
        "meshes/terrain/Tamriel/Objects/tamriel.4.-7.1.bto"
    );
}

#[test]
fn rnd_10_each_resident_object_block_stays_outside_full_resolution_radius() {
    let config = TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 3,
        lod_radius: 8,
        ..Default::default()
    };
    let mut streamer =
        TerrainLodStreamer::with_anchor(config, CellCoord::new(-3, 5)).unwrap();
    let focus = CellCoord::new(-3, 5);
    streamer.settle(focus);
    assert!(!streamer.resident_blocks().any(|block| {
        block.ring_distance(focus) <= config.full_res_radius
            || block.ring_distance(focus) > config.lod_radius
    }));
}

#[test]
fn rnd_10_focus_move_releases_old_blocks_and_loads_new_coverage() {
    let config = TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 2,
        lod_radius: 6,
        ..Default::default()
    };
    let mut streamer = TerrainLodStreamer::new(config).unwrap();
    let first = CellCoord::new(0, 0);
    streamer.settle(first);
    let previous = streamer.resident_blocks().collect::<std::collections::HashSet<_>>();
    let next = CellCoord::new(30, 0);
    let plan = streamer.update(next);
    assert!(!plan.load.is_empty());
    assert!(plan.unload.iter().any(|block| previous.contains(block)));
    assert!(plan.load.iter().all(|block| {
        block.ring_distance(next) > config.full_res_radius
            && block.ring_distance(next) <= config.lod_radius
    }));
}

#[test]
fn rnd_10_missing_bto_is_absent_without_fabricated_surfaces() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping missing BTO lookup");
        return;
    };
    let source = DataSource::open(&data).unwrap();
    let loaded = source
        .load_object_lod_block(
            GlobalFormId { file: 0, object: 0x3c },
            "RND10NoSuchWorldspace",
            LodBlockCoord {
                level: LodLevel::L4,
                x: 0,
                y: 0,
            },
        )
        .unwrap();
    assert!(loaded.is_none());
}

#[test]
fn rnd_10_installed_bto_surfaces_keep_the_resolved_world_identity() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed BTO identity check");
        return;
    };
    let source = DataSource::open(&data).unwrap();
    let world = GlobalFormId { file: 0, object: 0x3c };
    let mut found = None;
    'scan: for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        let span = level.cells() as usize;
        for y in (-64..=64).step_by(span) {
            for x in (-64..=64).step_by(span) {
                let block = LodBlockCoord {
                    level,
                    x: x as i32,
                    y: y as i32,
                };
                if let Some(loaded) = source
                    .load_object_lod_block(world, "Tamriel", block)
                    .unwrap()
                {
                    found = Some(loaded);
                    break 'scan;
                }
            }
        }
    }
    let loaded = found.expect("installed Tamriel data includes a BTO object block");
    assert!(!loaded.surfaces.is_empty());
    assert!(loaded.approx_bytes > 0);
    assert!(loaded.surfaces.iter().all(|surface| surface.reference == world));
}
