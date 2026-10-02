// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-10 boundary checks for the transition between near cells and BTO blocks.

use std::collections::HashSet;

use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel, TerrainLodConfig, TerrainLodStreamer};

#[test]
fn rnd_10_bto_and_near_cells_partition_the_requested_distance_band() {
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
        let mut streamer = TerrainLodStreamer::with_anchor(config, CellCoord::new(-3, 5)).unwrap();
        streamer.settle(focus);
        let resident: HashSet<LodBlockCoord> = streamer.resident_blocks().collect();

        for y in focus.y - config.lod_radius..=focus.y + config.lod_radius {
            for x in focus.x - config.lod_radius..=focus.x + config.lod_radius {
                let cell = CellCoord::new(x, y);
                if cell.ring_distance(focus) <= config.full_res_radius {
                    continue;
                }
                let owners = resident
                    .iter()
                    .filter(|block| {
                        x >= block.x
                            && x < block.x + block.level.cells()
                            && y >= block.y
                            && y < block.y + block.level.cells()
                    })
                    .count();
                assert_eq!(
                    owners,
                    1,
                    "cell ({x},{y}) at level {:?} has {owners} BTO owners outside near radius {}",
                    level,
                    config.full_res_radius
                );
            }
        }
    }
}
