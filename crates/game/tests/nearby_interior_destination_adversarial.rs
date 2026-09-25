// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-07 adversarial coverage for `World::nearby_interior_destination`
//! against the installed game. Gated on `SKYRIM_DATA`, skips cleanly
//! without it.
//!
//! No test exercised this function before this file: `doorway.rs`'s own
//! tests only cover `Threshold` plane crossing. `World::reference` is
//! `pub` and mutable, so these tests mutate already-loaded, real reference
//! state to build precise scenarios rather than fabricate plugin records.

use std::path::PathBuf;

use soe_esm::records::Transform;
use soe_esm::{GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;

const MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

fn real_game(data: &std::path::Path) -> Game {
    let mut order = LoadOrder::new();
    for name in MASTERS {
        let path = data.join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let plugin = Plugin::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        order.add(name, plugin).unwrap();
    }
    Game::new(order)
}

/// The Bannered Mare, loaded, with its own load doors (id, far door).
fn bannered_mare(
    data: &std::path::Path,
) -> (Game, GlobalFormId, Vec<(GlobalFormId, GlobalFormId)>) {
    let game = real_game(data);
    let cell = game
        .world_mut()
        .cell_by_editor_id("WhiterunBanneredMare")
        .expect("the Bannered Mare is in Skyrim.esm");
    let built = game.world_mut().load_cell(cell);
    assert!(built > 0, "the cell built no references");
    let doors: Vec<(GlobalFormId, GlobalFormId)> = game
        .world_mut()
        .cell_doors(cell)
        .into_iter()
        .filter_map(|(id, far)| far.map(|f| (id, f)))
        .collect();
    assert!(!doors.is_empty(), "no load door found in the cell");
    (game, cell, doors)
}

fn is_interior_cell(game: &Game, cell: GlobalFormId) -> bool {
    game.world_mut()
        .order()
        .cell_index()
        .expect("cell index built by cell_by_editor_id/load_cell")
        .is_interior(cell)
}

/// The walkthrough case: step out of the Bannered Mare, land in Whiterun,
/// and the return door has to be selected from the grid cell the player
/// stands in, even though vanilla files that door in another cell of the
/// same worldspace.
#[test]
fn hst_07_an_exterior_landing_selects_the_return_door_across_its_worldspace() {
    let Some(data) = data_dir() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let (game, cell, doors) = bannered_mare(&data);
    let (far_door, far_cell) = doors
        .iter()
        .find_map(|&(_, far)| {
            let far_cell = game.world_mut().reference(far).and_then(|r| r.cell)?;
            (!is_interior_cell(&game, far_cell)).then_some((far, far_cell))
        })
        .expect("the Bannered Mare has an exterior-leading door");
    game.world_mut().load_cell(far_cell);
    let position = game
        .world_mut()
        .reference(far_door)
        .expect("far door")
        .position;
    let (world, standing) = {
        let world = game.world_mut();
        let index = world.order().cell_index().expect("cell index");
        let owner = index
            .grid_of(far_cell)
            .map(|grid| grid.world)
            .or_else(|| {
                index
                    .cells()
                    .filter_map(|(id, _)| index.grid_of(id))
                    .map(|grid| grid.world)
                    .find(|&w| index.world_persistent_cell(w) == Some(far_cell))
            })
            .expect("the far door's cell belongs to a worldspace");
        let x = (position[0] / 4096.0).floor() as i32;
        let y = (position[1] / 4096.0).floor() as i32;
        let standing = index
            .exterior_cell(owner, x, y)
            .expect("grid cell under the door");
        (owner, standing)
    };
    game.world_mut().load_cell(standing);
    eprintln!("HST-07 world={world} far_cell={far_cell} standing={standing} door={far_door}");

    let found = game
        .world_mut()
        .nearby_interior_destination(standing, position, 500.0)
        .expect("no return door selected from the exterior landing");
    assert_eq!(found.cell, cell, "the return door leads somewhere else");

    // The same coordinates asked from inside an interior compare nothing
    // outside that interior.
    assert!(game
        .world_mut()
        .nearby_interior_destination(cell, position, 1.0)
        .is_none_or(|d| d.door != found.door));
}

#[test]
fn an_exterior_destination_is_never_selected_as_an_interior_prefetch_target() {
    let Some(data) = data_dir() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let (game, cell, doors) = bannered_mare(&data);

    // Ground truth is read from the same data the function would read,
    // never assumed from prose: find a door whose far side genuinely
    // resolves outside, rather than trusting that doors[0] is it.
    let mut exterior = None;
    for &(door, far_door) in &doors {
        let far_cell = game.world_mut().reference(far_door).and_then(|r| r.cell);
        if let Some(far_cell) = far_cell {
            if !is_interior_cell(&game, far_cell) {
                exterior = Some((door, far_cell));
                break;
            }
        }
    }
    let (door, far_cell) = exterior
        .expect("the Bannered Mare has no exterior-leading door in this install to test against");
    let door_position = game
        .world_mut()
        .reference(door)
        .expect("door resolves")
        .position;

    let found = game
        .world_mut()
        .nearby_interior_destination(cell, door_position, 1.0);
    assert!(
        found.is_none(),
        "an exterior destination (cell {far_cell:?}) was selected as an \
         interior prefetch target: {found:?}"
    );
}

#[test]
fn a_disabled_or_deleted_door_is_excluded_even_with_a_valid_interior_link_in_range() {
    let Some(data) = data_dir() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let (game, cell, doors) = bannered_mare(&data);
    let (door, far_door) = doors[0];

    // A second interior cell, distinct from the Bannered Mare, taken
    // straight from the load order's own cell index rather than a second
    // named editor id.
    let other_interior = {
        let world = game.world_mut();
        let index = world
            .order()
            .cell_index()
            .expect("cell index built by cell_by_editor_id");
        let mut candidates: Vec<GlobalFormId> = index
            .cells()
            .filter(|&(id, _)| id != cell && index.is_interior(id))
            .map(|(id, _)| id)
            .collect();
        candidates.sort_unstable();
        candidates
            .first()
            .copied()
            .expect("the load order has more than one interior cell")
    };

    // Make the door's own far side genuinely interior, without touching
    // anything the code under test would read from a plugin record: only
    // the resident reference's cached `.cell` is mutated.
    game.world_mut()
        .reference(far_door)
        .expect("far door resolves")
        .cell = Some(other_interior);
    let door_position = game
        .world_mut()
        .reference(door)
        .expect("door resolves")
        .position;

    let found = game
        .world_mut()
        .nearby_interior_destination(cell, door_position, 1.0);
    assert_eq!(
        found.map(|d| d.cell),
        Some(other_interior),
        "a door with a real interior link in range was not selected as a baseline"
    );

    // The door goes dark, e.g. a scripted secondary entrance shut for a
    // quest stage: must never be offered as a prefetch target even though
    // the authored link is otherwise identical to the baseline above.
    game.world_mut()
        .reference(door)
        .expect("door resolves")
        .enabled = false;
    let found = game
        .world_mut()
        .nearby_interior_destination(cell, door_position, 1.0);
    assert!(
        found.is_none(),
        "a disabled door was still selected as a prefetch target: {found:?}"
    );

    // Re-enabled but deleted (a mod's compatibility patch removing it):
    // same requirement.
    {
        let mut world = game.world_mut();
        let reference = world.reference(door).expect("door resolves");
        reference.enabled = true;
        reference.deleted = true;
    }
    let found = game
        .world_mut()
        .nearby_interior_destination(cell, door_position, 1.0);
    assert!(
        found.is_none(),
        "a deleted door was still selected as a prefetch target: {found:?}"
    );
}

#[test]
fn a_door_in_an_unrelated_cell_at_the_same_coordinates_is_never_selected() {
    let Some(data) = data_dir() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let (game, cell, doors) = bannered_mare(&data);
    let (door, _far_door) = doors[0];
    let door_position = game
        .world_mut()
        .reference(door)
        .expect("door resolves")
        .position;

    // An unrelated interior cell that has at least one door of its own,
    // found by walking the load order's cell index rather than a second
    // named editor id.
    let candidates: Vec<GlobalFormId> = {
        let world = game.world_mut();
        let index = world
            .order()
            .cell_index()
            .expect("cell index built by cell_by_editor_id");
        let mut candidates: Vec<GlobalFormId> = index
            .cells()
            .filter(|&(id, _)| id != cell && index.is_interior(id))
            .map(|(id, _)| id)
            .collect();
        candidates.sort_unstable();
        candidates
    };

    let mut chosen = None;
    for candidate in candidates.into_iter().take(60) {
        game.world_mut().load_cell(candidate);
        if let Some(&(other_door, _)) = game.world_mut().cell_doors(candidate).first() {
            chosen = Some(other_door);
            break;
        }
    }
    let other_door = chosen
        .expect("no unrelated interior cell with a door was found among the first 60 candidates");

    // Give the unrelated cell's own door the exact absolute coordinates of
    // the Bannered Mare's door, and a valid teleport link, so the only
    // thing distinguishing it from a real candidate is which cell it is
    // filed under. `nearby_interior_destination`'s own contract, in the
    // done check: "without comparing coordinates from unrelated interior
    // cells."
    {
        let mut world = game.world_mut();
        let reference = world.reference(other_door).expect("other door resolves");
        reference.enabled = true;
        reference.deleted = false;
        reference.position = door_position;
        reference.teleport = Some((door, Transform::default()));
    }

    let found = game
        .world_mut()
        .nearby_interior_destination(cell, door_position, 1.0);
    assert_ne!(
        found.map(|d| d.door),
        Some(other_door),
        "a door from an unrelated interior cell was selected because its raw \
         coordinates matched the query position, even though it is not filed \
         under the player's current cell"
    );
}
