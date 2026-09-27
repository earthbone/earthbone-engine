// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-15's planned check: the `WhiterunOrigin` radius 1 block has no
//! terrain edge where neighbouring cells disagree by more than one VHGT
//! sample, with the cause of the 2026-09-23 gaps named in
//! `docs/godot-host.md`.
//!
//! Judge-authored, kept separate from the breaker's
//! `terrain_seam_reconciliation_adversarial.rs`, which targets a specific
//! iteration-order corner bug rather than the plain done check.
//!
//! Run with SKYRIM_DATA set, headless, no windowed renderer needed:
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test terrain_seam_reconciliation_check -- --nocapture
//! ```

use std::path::PathBuf;

use soe_godot_host::data_source::DataSource;
use soe_godot_host::units::{LAND_HEIGHT_UNITS, LAND_SAMPLES, METRES_PER_UNIT};

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}")))
}

/// Height at row `row`, column `col` of a terrain surface's row-major
/// position list, y (Godot up) being the height component.
fn height_at(positions: &[[f32; 3]], row: usize, col: usize) -> f32 {
    positions[row * LAND_SAMPLES + col][1]
}

/// RND-15's done check, exercised directly against the shipped
/// `DataSource::stream_exterior` entry point rather than the private
/// `reconcile_seams` function: no shared edge in the block disagrees by
/// more than one VHGT sample (`LAND_HEIGHT_UNITS`, 8 world units, about
/// 0.114 m).
#[test]
fn rnd_15_whiterun_origin_block_has_no_seam_wider_than_one_sample() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping rnd_15_whiterun_origin_block_has_no_seam_wider_than_one_sample");
        return;
    };
    let loaded = source
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|e| panic!("stream_exterior: {e}"));

    // The block plan is 3 by 3 grid squares, radius 1 around the origin.
    assert_eq!(
        loaded.block.cells.len(),
        9,
        "WhiterunOrigin radius 1 must resolve exactly the 9-cell block the check targets"
    );
    // The done check's own premise: some cells in this block rely on a
    // parent worldspace's LAND, which is the source of the 2026-09-23 gap.
    assert!(
        loaded.block.land_parent.is_some(),
        "WhiterunWorld must carry a land parent (Tamriel) for the seam case to arise at all"
    );

    let mut grids: std::collections::HashMap<(i32, i32), Vec<[f32; 3]>> =
        std::collections::HashMap::default();
    for (cell, loaded_cell) in &loaded.cells {
        let Some(surface) = loaded_cell.surfaces.iter().find(|surface| {
            surface.arrays.positions.len() == LAND_SAMPLES * LAND_SAMPLES
                && surface.arrays.triangle_count() == 2048
                && surface.arrays.material.diffuse.is_none()
        }) else {
            continue;
        };
        grids.insert((cell.x, cell.y), surface.arrays.positions.clone());
    }
    assert_eq!(
        grids.len(),
        9,
        "every one of the 9 block cells must carry a terrain surface, own or parent LAND"
    );

    let tolerance = LAND_HEIGHT_UNITS * METRES_PER_UNIT;
    let mut worst: Option<(&str, i32, i32, usize, f32)> = None;
    for (&(x, y), here) in &grids {
        if let Some(east) = grids.get(&(x + 1, y)) {
            for row in 0..LAND_SAMPLES {
                let diff = (height_at(here, row, LAND_SAMPLES - 1) - height_at(east, row, 0)).abs();
                if diff > tolerance && worst.is_none_or(|(_, _, _, _, best)| diff > best) {
                    worst = Some(("east-west", x, y, row, diff));
                }
            }
        }
        if let Some(north) = grids.get(&(x, y + 1)) {
            for col in 0..LAND_SAMPLES {
                let diff =
                    (height_at(here, LAND_SAMPLES - 1, col) - height_at(north, 0, col)).abs();
                if diff > tolerance && worst.is_none_or(|(_, _, _, _, best)| diff > best) {
                    worst = Some(("north-south", x, y, col, diff));
                }
            }
        }
    }

    assert!(
        worst.is_none(),
        "seam wider than one sample survives reconcile_seams: {worst:?} \
         (edge, cell x, cell y, row/col, diff m), tolerance {tolerance} m"
    );
}

/// The other half of the done check: the cause of the 2026-09-23 gaps is
/// named where a reviewer will look, `docs/godot-host.md`'s RND-15
/// section. Read at compile time so a doc rewrite that drops the cause
/// fails this test without needing SKYRIM_DATA.
#[test]
fn rnd_15_cause_of_the_2026_09_23_gaps_is_named_in_the_exterior_doc() {
    let doc = include_str!("../../../docs/godot-host.md");
    let Some(section_start) = doc.find("RND-15") else {
        panic!("docs/godot-host.md must have an RND-15 section naming the seam gap's cause");
    };
    let section = &doc[section_start..];

    assert!(
        section.contains("Tamriel") && section.contains("WhiterunWorld"),
        "RND-15 section must name the two worldspaces whose LAND data disagreed at the seam"
    );
    assert!(
        section.contains("reconcile_seams"),
        "RND-15 section must name the fix function, reconcile_seams"
    );
    assert!(
        section.to_lowercase().contains("parent") || section.to_lowercase().contains("fallback"),
        "RND-15 section must name the land-parent fallback as the mechanism that pulls in \
         mismatched terrain"
    );
}
