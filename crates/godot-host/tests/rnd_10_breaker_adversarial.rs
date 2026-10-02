// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial RND-10 handoff coverage at the full-resolution boundary.
//!
//! TES5Edit's `TwbLodSettings.BlockForCell` assigns each object to a level
//! sized block. NIF's `BSSegmentedTriShape` describes a grid segmentation for
//! LOD. A handoff must cover cells outside the full-resolution radius while
//! avoiding a second draw over cells inside it.

use std::collections::HashSet;

use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel, TerrainLodConfig, TerrainLodStreamer};

#[test]
fn rnd_10_lod_blocks_stay_outside_near_geometry_and_cells_keep_one_block_identity() {
    let config = TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 2,
        lod_radius: 8,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
        budget_bytes: 0,
    };
    let focus = CellCoord::new(0, 0);
    let mut streamer = TerrainLodStreamer::new(config).unwrap();
    streamer.settle(focus);
    let resident: HashSet<LodBlockCoord> = streamer.resident_blocks().collect();
    assert!(!resident.is_empty(), "distant object LOD should be planned");

    for y in -config.lod_radius..=config.lod_radius {
        for x in -config.lod_radius..=config.lod_radius {
            let cell = CellCoord::new(x, y);
            let assigned = LodBlockCoord::containing(cell, LodLevel::L4);
            assert_eq!(assigned.level, LodLevel::L4);
            assert_eq!(assigned.x, x / LodLevel::L4.cells() * LodLevel::L4.cells());
            assert_eq!(assigned.y, y / LodLevel::L4.cells() * LodLevel::L4.cells());
        }
    }

    assert!(resident.iter().all(|block| {
        block.ring_distance(focus) > config.full_res_radius
            && block.ring_distance(focus) <= config.lod_radius
    }));
}
