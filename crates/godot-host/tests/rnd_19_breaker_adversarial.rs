// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-19 streaming recovery and repeated-focus edge cases.

use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodLevel, TerrainLodConfig, TerrainLodStreamer};

fn config() -> TerrainLodConfig {
    TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 2,
        lod_radius: 12,
        max_loads_per_tick: 1,
        max_unloads_per_tick: 1,
        budget_bytes: 0,
    }
}

#[test]
fn rnd_19_transient_block_failure_is_retried_at_the_same_focus() {
    let focus = CellCoord::new(0, 0);
    let mut streamer = TerrainLodStreamer::new(config()).unwrap();
    let first = streamer.step(focus, false, |_| {
        Err::<Option<u64>, _>("temporary read error")
    });
    let (block, _) = first.failed.first().expect("first requested block fails");
    assert!(first.loaded.is_empty());
    assert!(!streamer.is_resident(*block));

    let retry = streamer.update(focus);
    assert_eq!(retry.load, vec![*block]);
}

#[test]
fn rnd_19_missing_content_is_marked_absent_and_never_reloaded() {
    let focus = CellCoord::new(0, 0);
    let mut streamer = TerrainLodStreamer::new(config()).unwrap();
    let first = streamer.step(focus, false, |_| Ok::<Option<u64>, &str>(None));
    let block = *first.missing.first().expect("requested block is missing");
    assert_eq!(streamer.absent_count(), 1);

    for _ in 0..8 {
        let plan = streamer.update(focus);
        assert!(
            !plan.load.contains(&block),
            "missing block must stay absent"
        );
    }
}

#[test]
fn rnd_19_same_focus_keeps_loaded_blocks_without_more_io() {
    let focus = CellCoord::new(-29, 17);
    let mut streamer = TerrainLodStreamer::new(config()).unwrap();
    let first = streamer.step(focus, true, |_| Ok::<Option<u64>, &str>(Some(4096)));
    assert!(!first.loaded.is_empty());
    let resident = streamer
        .resident_blocks()
        .collect::<std::collections::HashSet<_>>();

    let mut calls = 0;
    let repeated = streamer.step(focus, false, |_| {
        calls += 1;
        Ok::<Option<u64>, &str>(Some(4096))
    });
    assert_eq!(calls, 0);
    assert!(repeated.loaded.is_empty());
    assert!(repeated.unloaded.is_empty());
    assert_eq!(
        streamer
            .resident_blocks()
            .collect::<std::collections::HashSet<_>>(),
        resident
    );
}

#[test]
fn rnd_19_lod_blocks_do_not_overlap_retained_full_cells() {
    let focus = CellCoord::new(0, 0);
    let mut streamer = TerrainLodStreamer::new(config()).unwrap();
    streamer.settle(focus);

    // This cell is outside the retained full-resolution radius, but its L4
    // BTR block also contains the focus cell and must not double draw it.
    let boundary_block =
        soe_godot_host::terrain_lod::LodBlockCoord::containing(CellCoord::new(3, 0), LodLevel::L4);
    assert!(
        !streamer.is_resident(boundary_block),
        "an LOD block overlapping retained full-resolution cells must stay unloaded"
    );
}

#[test]
fn rnd_19_budget_releases_do_not_repeat_until_focus_changes() {
    let focus = CellCoord::new(3, -4);
    let mut budget = config();
    budget.max_loads_per_tick = 0;
    budget.budget_bytes = 1;
    let mut streamer = TerrainLodStreamer::new(budget).unwrap();
    let first = streamer.step(focus, true, |_| Ok::<Option<u64>, &str>(Some(128)));
    assert!(!first.evicted.is_empty());
    assert!(!streamer.over_budget());
    assert_eq!(streamer.usage_bytes(), 0);

    let repeat = streamer.update(focus);
    assert!(
        repeat.load.is_empty(),
        "evicted blocks must not thrash at one focus"
    );
    let moved = streamer.update(CellCoord::new(focus.x + 1, focus.y));
    assert!(
        !moved.load.is_empty(),
        "a focus change may retry blocks previously held back by the budget"
    );
}
