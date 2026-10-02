// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-10 boundary checks for the transition between near cells and BTO blocks.

use std::collections::HashSet;

use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel, TerrainLodConfig, TerrainLodStreamer};

#[test]
fn rnd_10_each_cell_maps_to_one_source_defined_bto_block() {
    for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        let config = TerrainLodConfig {
            level,
            full_res_radius: 2,
            lod_radius: 2 + level.cells() * 2,
            max_loads_per_tick: 0,
            max_unloads_per_tick: 0,
            budget_bytes: 0,
        };
        let focus = CellCoord::new(1, -1);
        let anchor = CellCoord::new(-3, 5);
        let mut streamer = TerrainLodStreamer::with_anchor(config, anchor).unwrap();
        streamer.settle(focus);
        let resident: HashSet<LodBlockCoord> = streamer.resident_blocks().collect();

        for y in focus.y - config.lod_radius..=focus.y + config.lod_radius {
            for x in focus.x - config.lod_radius..=focus.x + config.lod_radius {
                let cell = CellCoord::new(x, y);
                let assigned = LodBlockCoord::containing_from(cell, level, anchor);
                let span = level.cells();
                assert_eq!(assigned.level, level);
                assert_eq!(assigned.x, anchor.x + (x - anchor.x) / span * span);
                assert_eq!(assigned.y, anchor.y + (y - anchor.y) / span * span);
            }
        }

        assert!(resident.iter().all(|block| {
            block.level == level
                && block.ring_distance(focus) > config.full_res_radius
                && block.ring_distance(focus) <= config.lod_radius
        }));
    }
}
