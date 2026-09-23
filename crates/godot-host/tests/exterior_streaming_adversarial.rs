// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial tests for PRF-02's exterior streaming fix
//! (`DataSource::stream_exterior_each`, reached here through the public
//! `DataSource::resolve_exterior_block` and `DataSource::stream_exterior`).
//! These attack the claim and the API contract, not the happy path already
//! covered by `real_exterior.rs`.
//!
//! `CellStreamerNode::stream_exterior` in `godot_layer.rs`, the actual
//! low-peak-memory caller the PRF-02 fix targets, is a GDExtension `#[func]`
//! reachable only from a live Godot process. Nothing in this crate's test
//! suite, headless or windowed, currently calls it. That is a real coverage
//! gap in the PRF-02 change: the RSS claim in `docs/godot-host.md` was
//! measured with scratch probes that were deleted before the change, and no
//! automated test exercises the code path the fix touches. This file cannot
//! close that gap (a fenced-off `#[func]` needs a Godot runtime), so it
//! attacks what the public Rust API around it does and does not guarantee.
//!
//! Run with SKYRIM_DATA set, headless, no windowed renderer needed:
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test exterior_streaming_adversarial -- --nocapture
//! ```

use std::path::PathBuf;

use soe_godot_host::data_source::DataSource;

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}")))
}

/// A negative radius is invalid input. The first reader turned it into an
/// empty block that reported success, so a streamed exterior with no cells
/// looked like a valid result. This test found that defect and now guards
/// the fix: both entry points reject the radius at the API boundary.
#[test]
fn negative_radius_is_rejected_at_the_api_boundary() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the real-exterior adversarial test");
        return;
    };
    assert!(
        source.resolve_exterior_block("WhiterunOrigin", -1).is_err(),
        "resolve_exterior_block must reject a negative radius"
    );
    assert!(
        source.stream_exterior("WhiterunOrigin", -1).is_err(),
        "stream_exterior must reject a negative radius"
    );
}

/// The block's doc comment promises cells "sorted south to north then west
/// to east" (`ExteriorCell` ordering, `resolve_exterior_block`). Two
/// independent calls against the same immutable `DataSource` must therefore
/// walk the same cells in the same order and report identical totals. This
/// is the equivalence check `stream_exterior_each`'s refactor needs and
/// does not have: the streaming rewrite touches `land_for_cells`, which
/// keys a `HashMap` by `GlobalFormId`, and a caller-visible dependency on
/// that map's iteration order anywhere in the per-cell path would make
/// results nondeterministic between otherwise identical runs.
#[test]
fn stream_exterior_is_deterministic_across_repeated_calls() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the real-exterior adversarial test");
        return;
    };
    let first = source
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("stream_exterior first call: {e}"));
    let second = source
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("stream_exterior second call: {e}"));

    assert_eq!(
        first.block.cells, second.block.cells,
        "grid order must be stable between calls"
    );
    assert_eq!(first.cells.len(), second.cells.len());
    assert_eq!(first.references(), second.references());
    assert_eq!(first.meshes_parsed(), second.meshes_parsed());
    assert_eq!(first.meshes_failed(), second.meshes_failed());
    assert_eq!(first.triangles(), second.triangles());
    assert_eq!(first.surfaces(), second.surfaces());
    assert_eq!(
        first.land_cells, second.land_cells,
        "land_cells must not depend on HashMap iteration order"
    );
    for ((cell_a, loaded_a), (cell_b, loaded_b)) in first.cells.iter().zip(second.cells.iter()) {
        assert_eq!(cell_a, cell_b, "per-cell grid order must match");
        assert_eq!(
            loaded_a.stats.triangles, loaded_b.stats.triangles,
            "cell ({}, {}) triangle count changed between identical calls",
            cell_a.x, cell_a.y
        );
        assert_eq!(
            loaded_a.surfaces.len(),
            loaded_b.surfaces.len(),
            "cell ({}, {}) surface count changed between identical calls",
            cell_a.x,
            cell_a.y
        );
    }
}

/// Radius 0 is the documented minimum a caller can request (bench.gd
/// rejects negatives and caps at 128 in the script layer). It is also the
/// off-by-one boundary for the streaming rewrite: a radius-0 block must
/// resolve to exactly the origin cell, one entry, not zero (an empty
/// range) and not more than one (an inclusive-bound slip). PRF-02's fix
/// changed how cells are walked (`stream_exterior_each`'s `for cell in
/// &block.cells` replaces the old build-then-read `Vec`); an off-by-one in
/// that loop would show up first at this boundary.
#[test]
fn radius_zero_resolves_to_exactly_the_origin_cell() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the real-exterior adversarial test");
        return;
    };
    let block = source
        .resolve_exterior_block("WhiterunOrigin", 0)
        .unwrap_or_else(|e| panic!("resolve_exterior_block(0): {e}"));
    assert_eq!(
        block.cells.len(),
        1,
        "radius 0 must resolve to exactly the origin cell"
    );
    assert_eq!(
        (block.cells[0].x, block.cells[0].y),
        block.origin,
        "the one cell at radius 0 must be the origin itself"
    );
    let streamed = source
        .stream_exterior("WhiterunOrigin", 0)
        .unwrap_or_else(|e| panic!("stream_exterior(0): {e}"));
    assert_eq!(streamed.cells.len(), 1);
    assert!(
        streamed.references() > 0,
        "the origin cell should carry references, an empty result at \
         radius 0 would mean the loop never ran"
    );
}

/// Coverage for the actual low-peak-memory path PRF-02's fix added.
///
/// `CellStreamerNode::stream_exterior` (`godot_layer.rs`) is a GDExtension
/// `#[func]` reachable only from a live Godot process: nothing headless
/// can call it. What it does beyond a thin RenderingServer upload wrapper
/// is call `DataSource::stream_exterior_each` with a closure, one cell at
/// a time, instead of collecting the block into a `Vec` first the way
/// `stream_exterior_block` (exercised by `real_exterior.rs`) does. This
/// test calls `stream_exterior_each` directly, the same entry point
/// `godot_layer.rs` uses, and checks it against `stream_exterior_block`'s
/// own totals: same cell count and order, same land cell count, same
/// triangle total, and the callback invoked exactly once per cell in
/// `block.cells` order, matching the "one cell alive, then dropped"
/// contract the RSS fix depends on. It does not and cannot cover the
/// RenderingServer upload itself, that half needs a live Godot process.
#[test]
fn stream_exterior_each_matches_the_batching_path_it_replaces_in_godot_layer() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the real-exterior adversarial test");
        return;
    };
    let block = source
        .resolve_exterior_block("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("resolve_exterior_block(1): {e}"));
    let whole_block = source
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("stream_exterior(1): {e}"));

    let mut seen: Vec<(i32, i32)> = Vec::new();
    let mut land_cells = 0;
    let mut triangles: usize = 0;
    source
        .stream_exterior_each(&block, |cell, loaded, has_land| {
            seen.push((cell.x, cell.y));
            if has_land {
                land_cells += 1;
            }
            triangles += loaded.stats.triangles;
        })
        .unwrap_or_else(|e| panic!("stream_exterior_each: {e}"));

    let expected_order: Vec<(i32, i32)> = block.cells.iter().map(|cell| (cell.x, cell.y)).collect();
    assert_eq!(
        seen, expected_order,
        "stream_exterior_each must call on_cell once per cell, in block.cells order, \
         the same order the Godot host relies on for its cell_list"
    );
    assert_eq!(
        land_cells, whole_block.land_cells,
        "land cell count must match the non-streaming stream_exterior_block path"
    );
    assert_eq!(
        triangles,
        whole_block.triangles(),
        "per-cell triangle totals must match between the streaming and batched paths"
    );
}
