// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Judge-written check test for RND-03's done check, "Terrain LOD replaces
//! distant full resolution terrain within a recorded residency budget".
//!
//! Every clause this file can reach is exercised at the pure Rust data path:
//! `TerrainLodStreamer` planning LOD blocks strictly beyond the full
//! resolution radius and never inside it (the "replaces distant" clause at
//! the decision level), and a real BTR read through
//! `DataSource::load_lod_block` feeding `record_bytes` so `over_budget` and
//! `evict_to_budget` enforce a budget against real recorded sizes rather than
//! synthetic ones (the "recorded residency budget" clause).
//!
//! What this file cannot reach, and the companion
//! `godot/rnd_03_terrain_lod_check.gd` records as a failure: nothing in
//! `crates/godot-host/src` calls `TerrainLodStreamer` or `load_lod_block`
//! from `CellStreamerNode` or any other GDExtension-exposed type (confirmed
//! by grep, see the judge's report), so no full resolution terrain is ever
//! actually replaced in a running scene. The done check asks for that
//! replacement to happen, not only for the pieces it would be built from to
//! work in isolation.
//!
//! ```text
//! cargo test -p soe-godot-host --test rnd_03_terrain_lod_check
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test rnd_03_terrain_lod_check -- --nocapture
//! ```

use std::path::PathBuf;

use soe_esm::GlobalFormId;
use soe_godot_host::data_source::DataSource;
use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel, TerrainLodConfig, TerrainLodStreamer};

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}")))
}

/// Declared budget for the synthetic planning check: a streamer configured
/// with `full_res_radius` 2 and `lod_radius` 10 at `LodLevel::L4` must plan
/// to load only blocks whose ring distance from the focus exceeds 2, and
/// must never plan a block whose ring distance is 2 or less, no matter how
/// far `settle` runs. This is the "replaces distant" clause: LOD blocks are
/// only ever proposed beyond the radius full resolution terrain keeps.
#[test]
fn rnd_03_lod_plan_never_covers_the_full_resolution_radius() {
    let config = TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 2,
        lod_radius: 10,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
        budget_bytes: 0,
    };
    let mut streamer =
        TerrainLodStreamer::new(config).unwrap_or_else(|e| panic!("TerrainLodStreamer::new: {e}"));
    let focus = CellCoord::new(0, 0);
    let plan = streamer.settle(focus);

    assert!(
        !plan.load.is_empty(),
        "a 10 cell LOD radius around the origin at level 4 must plan at least one block"
    );
    for block in &plan.load {
        let d = block.ring_distance(focus);
        assert!(
            d > config.full_res_radius,
            "block {block:?} was planned for load at ring distance {d}, which does not \
             exceed full_res_radius {}: it overlaps the full resolution radius instead of \
             standing in for terrain beyond it",
            config.full_res_radius,
        );
        assert!(
            d <= config.lod_radius,
            "block {block:?} was planned for load at ring distance {d}, past lod_radius {}",
            config.lod_radius,
        );
    }
    for block in streamer.resident_blocks() {
        let d = block.ring_distance(focus);
        assert!(
            d > config.full_res_radius,
            "block {block:?} ended up resident at ring distance {d}, inside full_res_radius {}",
            config.full_res_radius,
        );
    }
}

/// Declared budget for the real-data accounting check: 3 resident BTR
/// blocks' worth of bytes, deliberately capped below what 3 real level 4
/// blocks around Tamriel's origin actually cost, so eviction has real work
/// to do. Skips cleanly without SKYRIM_DATA, per repository convention.
///
/// Loads real BTR blocks through `DataSource::load_lod_block`, records their
/// actual `approx_bytes` into a `TerrainLodStreamer`, and checks the budget
/// clause end to end against real sizes: `over_budget` reports true only
/// once real recorded bytes exceed the cap, and `evict_to_budget` brings
/// `usage_bytes` back at or under the declared cap while leaving evicted
/// blocks no longer resident.
#[test]
fn rnd_03_residency_budget_enforced_against_real_btr_bytes() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the real terrain LOD budget check");
        return;
    };
    let world = GlobalFormId {
        file: 0,
        object: 0x3c,
    };
    // Level 4 blocks around the one confirmed present in this machine's
    // `Skyrim - Meshes0.bsa` (meshes/terrain/tamriel/, x=-68, y=-36, see
    // `terrain_lod_real_data_adversarial.rs::known_present_block`), scanned
    // rather than hardcoded since only that single coordinate is confirmed
    // and Tamriel's LOD coverage is not dense at every neighboring block.
    // `load_lod_block` returns `Ok(None)` for a block with no BTR file, so a
    // scan just skips those, it never errors.
    let mut loaded_bytes = Vec::new();
    'scan: for y in -37..=-35 {
        for x in -69..=-64 {
            let block = LodBlockCoord {
                level: LodLevel::L4,
                x,
                y,
            };
            let Some(loaded) = source
                .load_lod_block(world, "tamriel", block)
                .unwrap_or_else(|e| panic!("load_lod_block at {block:?}: {e}"))
            else {
                continue;
            };
            assert!(
                loaded.approx_bytes > 0,
                "block {block:?} built {} surfaces but recorded 0 bytes",
                loaded.surfaces.len(),
            );
            loaded_bytes.push((block, loaded.approx_bytes));
            if loaded_bytes.len() == 3 {
                break 'scan;
            }
        }
    }
    assert!(
        loaded_bytes.len() >= 2,
        "need at least 2 real present level 4 blocks near (-68, -36) to exercise a real \
         budget, found {}",
        loaded_bytes.len(),
    );
    let total: u64 = loaded_bytes.iter().map(|(_, b)| b).sum();

    // Declared budget: half of what these three real blocks actually cost,
    // so the cap is guaranteed real work for eviction, not a number picked
    // to trivially pass.
    let budget_bytes = total / 2;
    assert!(
        budget_bytes > 0,
        "three real BTR blocks must cost enough bytes for a nonzero half-budget, got {total} total"
    );

    let config = TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 0,
        lod_radius: 4,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
        budget_bytes,
    };
    let mut streamer =
        TerrainLodStreamer::new(config).unwrap_or_else(|e| panic!("TerrainLodStreamer::new: {e}"));
    let focus = CellCoord::new(-68 * 4, -36 * 4);
    for (block, bytes) in &loaded_bytes {
        streamer.record_bytes(*block, *bytes);
    }

    assert_eq!(
        streamer.usage_bytes(),
        total,
        "recorded usage must match the real bytes loaded"
    );
    assert!(
        streamer.over_budget(),
        "usage_bytes {} must exceed the declared budget {budget_bytes}",
        streamer.usage_bytes(),
    );

    let evicted = streamer.evict_to_budget(focus);
    assert!(
        !evicted.is_empty(),
        "a budget set to half the real total must evict something"
    );
    assert!(
        streamer.usage_bytes() <= budget_bytes,
        "evict_to_budget must leave usage_bytes ({}) at or under budget_bytes ({budget_bytes})",
        streamer.usage_bytes(),
    );
    assert!(
        !streamer.over_budget(),
        "over_budget must clear once usage fits the cap"
    );
    for block in &evicted {
        assert!(
            !streamer.is_resident(*block),
            "evicted block {block:?} must no longer report resident",
        );
    }
}
