// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial tests for RND-15's seam fix (`reconcile_seams` in
//! `crates/godot-host/src/data_source/exterior.rs`, reached here only
//! through the public `DataSource::stream_exterior`, since the function
//! itself is private to its module).
//!
//! `reconcile_seams` walks each block position once and, for a cell that
//! has both an east neighbour and a north neighbour, runs two separate
//! passes against the same `heights` map: first east-west against the
//! east neighbour, then north-south against the north neighbour. Both
//! passes can write the very same array index, the cell's north-east
//! corner sample, since it sits on both the east edge (row 0 to 32 of the
//! shared column) and the north edge (column 0 to 32 of the shared row).
//! The north-south pass re-reads the map after the east-west pass already
//! wrote it, so the corner's final value is pulled toward the north
//! neighbour using a value that already moved toward the east neighbour,
//! while the east neighbour's own copy of that corner keeps the earlier,
//! now-stale east-west average. `reconcile_seams`'s own doc comment
//! promises every shared edge agrees after this runs. A corner where the
//! two passes disagree breaks that promise at exactly the point most
//! likely to show as a sky-coloured gap in `pan_capture.gd`, since a
//! quad's corner vertex is shared by every triangle that touches it.
//!
//! Run with SKYRIM_DATA set, headless, no windowed renderer needed:
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test terrain_seam_reconciliation_adversarial -- --nocapture
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use soe_godot_host::data_source::{DataSource, LoadedExterior};
use soe_godot_host::units::{LAND_HEIGHT_UNITS, LAND_SAMPLES, METRES_PER_UNIT};

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}")))
}

/// One VHGT sample step in meters, the done check's tolerance: "no
/// terrain edge where neighbouring cells disagree by more than one
/// sample."
fn seam_tolerance_m() -> f32 {
    LAND_HEIGHT_UNITS * METRES_PER_UNIT
}

/// Each streamed cell's terrain surface, as a row-major grid of Godot-space
/// positions (row 0 south, row `LAND_SAMPLES - 1` north, col 0 west, col
/// `LAND_SAMPLES - 1` east), keyed by grid position. A cell with no land
/// (no `LAND` record reachable, own or inherited) is simply absent, the
/// same way `reconcile_seams` treats it.
fn terrain_grids(loaded: &LoadedExterior) -> HashMap<(i32, i32), Vec<[f32; 3]>> {
    let mut out = HashMap::new();
    for (cell, loaded_cell) in &loaded.cells {
        let Some(surface) = loaded_cell.surfaces.iter().find(|surface| {
            surface.arrays.positions.len() == LAND_SAMPLES * LAND_SAMPLES
                && surface.arrays.triangle_count() == 2048
                && surface.arrays.material.diffuse.is_none()
        }) else {
            continue;
        };
        assert_eq!(
            surface.arrays.positions.len(),
            LAND_SAMPLES * LAND_SAMPLES,
            "cell ({}, {}) terrain surface must carry one position per VHGT sample",
            cell.x,
            cell.y
        );
        out.insert((cell.x, cell.y), surface.arrays.positions.clone());
    }
    out
}

/// Height in meters (Godot's up axis) at row `row`, column `col` of a
/// terrain grid returned by `terrain_grids`.
fn height_at(grid: &[[f32; 3]], row: usize, col: usize) -> f32 {
    grid[row * LAND_SAMPLES + col][1]
}

/// RND-15's done check: the `WhiterunOrigin` radius 1 block has no terrain
/// edge where neighbouring cells disagree by more than one sample.
/// `reconcile_seams`'s own doc comment makes the same promise for every
/// shared edge in the block, not only the plain mid-edge samples: every
/// vertex from row/col 0 to `LAND_SAMPLES - 1` on a shared edge is
/// supposed to match, and that range includes the corner vertex two edges
/// meet at.
#[test]
fn no_shared_edge_vertex_in_the_whiterun_block_disagrees_by_more_than_one_sample() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the seam-reconciliation adversarial test");
        return;
    };
    let loaded = source
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("stream_exterior: {e}"));
    let grids = terrain_grids(&loaded);
    assert_eq!(grids.len(), 9, "the seam survey must check all nine cells");
    let tolerance = seam_tolerance_m();

    let mut worst: Option<(&str, i32, i32, usize, f32)> = None;
    let mut record_worst = |edge: &'static str, x: i32, y: i32, idx: usize, diff: f32| {
        if worst.is_none_or(|(_, _, _, _, best)| diff > best) {
            worst = Some((edge, x, y, idx, diff));
        }
    };

    for (&(x, y), here) in &grids {
        if let Some(east) = grids.get(&(x + 1, y)) {
            for row in 0..LAND_SAMPLES {
                let a = height_at(here, row, LAND_SAMPLES - 1);
                let b = height_at(east, row, 0);
                let diff = (a - b).abs();
                if diff > tolerance {
                    record_worst("east-west", x, y, row, diff);
                }
            }
        }
        if let Some(north) = grids.get(&(x, y + 1)) {
            for col in 0..LAND_SAMPLES {
                let a = height_at(here, LAND_SAMPLES - 1, col);
                let b = height_at(north, 0, col);
                let diff = (a - b).abs();
                if diff > tolerance {
                    record_worst("north-south", x, y, col, diff);
                }
            }
        }
    }

    assert!(
        worst.is_none(),
        "seam left open after reconcile_seams: {worst:?} exceeds the one-sample \
         tolerance of {tolerance} m (edge, cell x, cell y, row/col index, diff in m)"
    );
}

/// Corners are where the compounding bug in the summary above should show
/// up sharpest: a cell that has both an east and a north neighbour inside
/// the block has its north-east corner sample written twice by
/// `reconcile_seams`, once per pass, and the second write uses the first
/// write's already-changed value instead of the original decoded VHGT
/// sample. This isolates that one vertex per interior grid position and
/// checks every cell that claims to own a copy of it, not just the pair
/// `reconcile_seams` happened to compare last.
#[test]
fn interior_corners_agree_across_every_cell_that_shares_them() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the seam-reconciliation adversarial test");
        return;
    };
    let loaded = source
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("stream_exterior: {e}"));
    let grids = terrain_grids(&loaded);
    let tolerance = seam_tolerance_m();
    let top = LAND_SAMPLES - 1;

    let mut mismatches = Vec::new();
    for &(x, y) in grids.keys() {
        // The corner shared by (x, y)'s NE vertex, (x+1, y)'s NW vertex,
        // (x, y+1)'s SE vertex and (x+1, y+1)'s SW vertex.
        let mut owners: Vec<((i32, i32), f32)> = Vec::new();
        if let Some(g) = grids.get(&(x, y)) {
            owners.push(((x, y), height_at(g, top, top)));
        }
        if let Some(g) = grids.get(&(x + 1, y)) {
            owners.push(((x + 1, y), height_at(g, top, 0)));
        }
        if let Some(g) = grids.get(&(x, y + 1)) {
            owners.push(((x, y + 1), height_at(g, 0, top)));
        }
        if let Some(g) = grids.get(&(x + 1, y + 1)) {
            owners.push(((x + 1, y + 1), height_at(g, 0, 0)));
        }
        if owners.len() < 2 {
            continue;
        }
        let (_, first_h) = owners[0];
        for &(cell, h) in &owners[1..] {
            let diff = (h - first_h).abs();
            if diff > tolerance {
                mismatches.push((owners[0].0, cell, diff));
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "cells that share a corner vertex must report the same reconciled height \
         within one sample ({tolerance} m), mismatches (cell a, cell b, diff m): \
         {mismatches:?}"
    );
}

/// `reconcile_seams` decides processing order from a `HashMap`'s key
/// iteration, and the standard library gives no order guarantee there (a
/// fresh `RandomState` seed per map). Two independent `DataSource::open`
/// calls against the same install therefore build two independent
/// `heights` maps, and if a cell's final corner value depends on which of
/// its two competing passes ran last, that value can differ between the
/// two runs even though the source LAND records are identical. Terrain
/// that changes shape between sessions on unchanged data breaks any cache,
/// diff or screenshot comparison built on top of it, `pan_capture.gd`'s
/// own regression check included.
#[test]
fn reconciled_terrain_is_identical_across_independent_data_source_opens() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the seam-reconciliation adversarial test");
        return;
    };
    let source_a = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open (a): {e}"));
    let source_b = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open (b): {e}"));
    let loaded_a = source_a
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("stream_exterior (a): {e}"));
    let loaded_b = source_b
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("stream_exterior (b): {e}"));
    let grids_a = terrain_grids(&loaded_a);
    let grids_b = terrain_grids(&loaded_b);

    assert_eq!(
        grids_a.keys().collect::<std::collections::BTreeSet<_>>(),
        grids_b.keys().collect::<std::collections::BTreeSet<_>>(),
        "the two opens must resolve the same set of terrain cells"
    );

    // Exact reproducibility, not seam tolerance: the same LAND bytes
    // decoded twice must reconcile to the same heights, whichever process
    // order the HashMap iteration happened to pick.
    let epsilon = 1e-4;
    let mut mismatches = Vec::new();
    for (&(x, y), grid_a) in &grids_a {
        let other = &grids_b[&(x, y)];
        for i in 0..grid_a.len() {
            let diff = (grid_a[i][1] - other[i][1]).abs();
            if diff > epsilon {
                mismatches.push((x, y, i, diff));
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "reconciled terrain differed between two independent DataSource::open calls \
         on the same install, (cell x, cell y, vertex index, diff m): {mismatches:?}"
    );
}
