// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial tests for RND-11's done check: "A LAND grass fixture
//! creates deterministic placement batches and releases them as cells
//! unload." `grass_surfaces_for_cell` (`data_source/grass.rs`) is
//! `pub(super)`, unreachable from an external test crate, so these attack
//! it through the public streaming API it is wired into
//! (`DataSource::stream_exterior`, `resolve_exterior_block`), mirroring
//! `exterior_streaming_adversarial.rs`'s pattern.
//!
//! Run with SKYRIM_DATA set, headless:
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test grass_streaming_adversarial -- --nocapture
//! ```

use std::path::PathBuf;

use soe_godot_host::data_source::DataSource;

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}")))
}

/// `grass.rs`'s own doc comment states the contract this checks: "Every
/// input to the placement hash is a stable coordinate ... No wall clock
/// and no global random state, so streaming the same cell twice produces
/// the same batch, which is what the release-on-unload path and the
/// RND-11 done check both rely on." Two `stream_exterior` calls on the
/// same open `DataSource` must therefore place the same grass count per
/// cell.
#[test]
fn grass_placement_is_deterministic_across_repeated_stream_calls() {
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

    assert_eq!(first.cells.len(), second.cells.len());
    for ((cell_a, loaded_a), (cell_b, loaded_b)) in first.cells.iter().zip(second.cells.iter()) {
        assert_eq!(cell_a, cell_b, "per-cell grid order must match");
        assert_eq!(
            loaded_a.stats.grass_placements, loaded_b.stats.grass_placements,
            "cell ({}, {}) grass placement count changed between identical calls",
            cell_a.x, cell_a.y
        );
    }
}

/// The same determinism claim, checked across two independent
/// `DataSource::open` calls instead of one open source streamed twice.
/// The seed in `grass.rs` is built only from the cell's grid position,
/// quadrant, height grid cell and instance index, none of which a fresh
/// process, a fresh file handle set or a fresh worker pool should change.
/// A dependency on anything opened fresh per process (an address, a file
/// handle order, a thread id) would pass the repeated-call test above and
/// still fail here.
#[test]
fn grass_placement_is_deterministic_across_independent_data_source_opens() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the real-exterior adversarial test");
        return;
    };
    let source_a = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open (a): {e}"));
    let source_b = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open (b): {e}"));
    let a = source_a
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("stream_exterior (a): {e}"));
    let b = source_b
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("stream_exterior (b): {e}"));

    assert_eq!(a.cells.len(), b.cells.len());
    for ((cell_a, loaded_a), (cell_b, loaded_b)) in a.cells.iter().zip(b.cells.iter()) {
        assert_eq!(cell_a, cell_b, "per-cell grid order must match");
        assert_eq!(
            loaded_a.stats.grass_placements, loaded_b.stats.grass_placements,
            "cell ({}, {}) grass placement count differs between independently \
             opened DataSources, the seed must depend only on stable coordinates",
            cell_a.x, cell_a.y
        );
    }
}

/// `grass.rs` names `MAX_GRASS_PLACEMENTS_PER_CELL` as "the 'bounded' in
/// RND-11's title" and caps it at 512, breaking out of the per-cell
/// placement loop once reached. This is the "bounded instance batches"
/// half of the done check: no cell's batch may exceed that documented cap,
/// however densely its LAND texture layers are painted.
#[test]
fn grass_placements_never_exceed_the_per_cell_budget() {
    const MAX_GRASS_PLACEMENTS_PER_CELL: usize = 512;
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the real-exterior adversarial test");
        return;
    };
    let block = source
        .stream_exterior("WhiterunOrigin", 2)
        .unwrap_or_else(|e| panic!("stream_exterior(2): {e}"));
    for (cell, loaded) in &block.cells {
        assert!(
            loaded.stats.grass_placements <= MAX_GRASS_PLACEMENTS_PER_CELL,
            "cell ({}, {}) placed {} grass instances, over grass.rs's documented \
             per-cell cap of {MAX_GRASS_PLACEMENTS_PER_CELL}",
            cell.x,
            cell.y,
            loaded.stats.grass_placements
        );
    }
}

/// The "releases them as cells unload" half of the done check. Shrinking
/// the streamed radius around the same origin must drop the neighbour
/// cells entirely, including whatever grass they carried, not retain them
/// alongside the smaller block. This exercises the same
/// `resolve_exterior_block`/`stream_exterior` boundary
/// `exterior_streaming_adversarial.rs`'s `radius_zero_resolves_to_exactly_the_origin_cell`
/// checks, but asserts specifically on the grass batches the wider radius
/// found, not just cell count.
#[test]
fn shrinking_the_radius_releases_cells_and_their_grass() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the real-exterior adversarial test");
        return;
    };
    let wide = source
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("stream_exterior(1): {e}"));
    let narrow = source
        .stream_exterior("WhiterunOrigin", 0)
        .unwrap_or_else(|e| panic!("stream_exterior(0): {e}"));

    assert_eq!(
        narrow.cells.len(),
        1,
        "radius 0 must resolve to exactly the origin cell"
    );
    let origin = narrow.cells[0].0;

    let released: Vec<_> = wide
        .cells
        .iter()
        .filter(|(cell, _)| *cell != origin)
        .collect();
    assert!(
        !released.is_empty(),
        "radius 1 around WhiterunOrigin must include neighbour cells, otherwise \
         this test cannot show anything being released"
    );
    let released_grass: usize = released
        .iter()
        .map(|(_, loaded)| loaded.stats.grass_placements)
        .sum();
    assert!(
        released_grass > 0,
        "at least one of WhiterunOrigin's radius 1 neighbours must carry grass, \
         otherwise this test cannot show grass batches being released"
    );
    assert!(
        narrow.cells.iter().all(|(cell, _)| *cell == origin),
        "a radius 0 stream must not carry any of the radius 1 neighbour cells, \
         they must be released, not retained"
    );
}
