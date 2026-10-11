// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

#[path = "transitions.rs"]
#[allow(dead_code)]
mod transitions;

use transitions::{game, id};

const CELL_B: u32 = 0x811;
const ROCK_B: u32 = 0x840;

#[test]
fn hst_13_enabling_reference_in_resident_cell_reports_its_loaded_3d() {
    let game = game();
    let cell = id(CELL_B);
    let reference = id(ROCK_B);

    game.world_mut().reference(reference).unwrap().enabled = false;
    game.world_mut().attach_streaming_cells(&[cell]).unwrap();
    assert!(!game.world().is_reference_3d_loaded(reference));

    game.world_mut().set_reference_enabled(reference, true);
    assert!(game.world().is_reference_3d_loaded(reference));
}

#[test]
fn hst_13_missing_reference_never_reports_loaded_3d() {
    let game = game();
    let cell = id(CELL_B);
    let missing = id(0x8ff);

    game.world_mut().attach_streaming_cells(&[cell]).unwrap();
    assert!(!game.world().is_reference_3d_loaded(missing));
}

#[test]
fn hst_13_rejected_duplicate_arrival_preserves_existing_residency() {
    let game = game();
    let cell = id(CELL_B);
    let reference = id(ROCK_B);

    game.world_mut().attach_streaming_cells(&[cell]).unwrap();
    assert!(game.world().is_reference_3d_loaded(reference));
    assert!(game
        .world_mut()
        .attach_streaming_cells(&[cell, cell])
        .is_err());
    assert!(game.world().is_reference_3d_loaded(reference));
}
