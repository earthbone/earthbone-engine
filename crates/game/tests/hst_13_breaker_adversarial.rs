// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "transitions.rs"]
#[allow(dead_code)]
mod transitions;

use transitions::{game, id};

const CELL_A: u32 = 0x810;
const CELL_B: u32 = 0x811;
const ROCK_B: u32 = 0x840;

#[test]
fn hst_13_focus_arrival_publishes_prefetched_reference_3d() {
    let mut game = game();
    let reference = id(ROCK_B);

    game.world_mut().prefetch_cell(id(CELL_B));
    assert!(!game.world().is_reference_3d_loaded(reference));

    game.change_cell(Some(id(CELL_B)));
    assert!(game.world().is_reference_3d_loaded(reference));
}

#[test]
fn hst_13_neighbor_departure_clears_3d_but_keeps_changed_reference() {
    let mut game = game();
    let reference = id(ROCK_B);

    game.world_mut()
        .attach_streaming_cells(&[id(CELL_B)])
        .unwrap();
    assert!(game.world().is_reference_3d_loaded(reference));
    game.world_mut().reference(reference).unwrap().position[0] += 1.0;

    game.world_mut()
        .detach_streaming_cells(&[id(CELL_B)])
        .unwrap();
    assert!(game.world().is_reference_loaded(reference));
    assert!(!game.world().is_reference_3d_loaded(reference));
}

#[test]
fn hst_13_prefetch_and_uninstantiated_queries_are_not_3d_residency() {
    let mut game = game();
    let missing = id(0x8ff);

    game.world_mut().prefetch_cell(id(CELL_B));
    assert!(!game.world().is_reference_3d_loaded(id(ROCK_B)));
    assert!(!game.world().is_reference_3d_loaded(missing));
}

#[test]
fn hst_13_invalid_host_delta_leaves_existing_residency_unchanged() {
    let mut game = game();
    let reference = id(ROCK_B);

    game.change_cell(Some(id(CELL_B)));
    assert!(game.world().is_reference_3d_loaded(reference));
    assert!(game
        .update_streaming_cells(id(CELL_B), &[id(CELL_B), id(CELL_B)], &[])
        .is_err());
    assert!(game.world().is_reference_3d_loaded(reference));
}

#[test]
fn hst_13_focus_stream_out_clears_3d() {
    let mut game = game();
    let reference = id(ROCK_B);

    game.change_cell(Some(id(CELL_B)));
    assert!(game.world().is_reference_3d_loaded(reference));
    game.change_cell(None);
    assert!(!game.world().is_reference_3d_loaded(reference));
}

#[test]
fn hst_13_focus_transition_clears_old_cell_and_keeps_destination_loaded() {
    let mut game = game();
    let old_reference = id(0x830);
    game.change_cell(Some(id(CELL_A)));
    game.change_cell(Some(id(CELL_B)));

    assert!(!game.world().is_reference_3d_loaded(old_reference));
    assert!(game.world().is_reference_3d_loaded(id(ROCK_B)));
}

#[test]
fn hst_13_position_changes_do_not_clear_resident_3d() {
    let game = game();
    let reference = id(ROCK_B);
    game.world_mut().attach_streaming_cells(&[id(CELL_B)]).unwrap();

    let mut world = game.world_mut();
    let rock = world.reference(reference).unwrap();
    rock.position = [f32::MAX, -f32::MAX, 0.0];
    drop(world);

    assert!(
        game.world().is_reference_3d_loaded(reference),
        "CK Is3DLoaded remains true for resident 3D outside rendered room bounds"
    );
}
