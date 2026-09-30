// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! A real load door on the installed game: walk out of the Bannered Mare and
//! report what moved. Gated on `SKYRIM_DATA`, skips cleanly without it.
//!
//! The Bannered Mare is the cell the rest of the suite exercises most, so its
//! numbers are already known from the streaming and collision work and a
//! surprise here is worth chasing.

#[path = "common/installed.rs"]
mod installed;

use std::path::PathBuf;
use std::time::Instant;

use soe_esm::GlobalFormId;
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

fn real_game(_data: &std::path::Path) -> Game {
    let order = installed::load_order(&MASTERS).unwrap();
    Game::from_shared_order(order)
}

#[test]
fn the_bannered_mare_door_leads_outside() {
    let Some(data) = data_dir() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let mut game = real_game(&data);

    let cell = game
        .world_mut()
        .cell_by_editor_id("WhiterunBanneredMare")
        .expect("the Bannered Mare is in Skyrim.esm");

    let started = Instant::now();
    let built = game.world_mut().load_cell(cell);
    let load_time = started.elapsed();
    assert!(built > 0, "the cell built no references");

    // Every load door in the cell: a reference whose base is a DOOR and which
    // carries teleport data. A tavern has at least the one onto the street.
    let doors: Vec<(GlobalFormId, GlobalFormId)> = game
        .world_mut()
        .cell_doors(cell)
        .into_iter()
        .filter_map(|(id, far)| far.map(|f| (id, f)))
        .collect();
    eprintln!("Bannered Mare, cell {cell:?}");
    eprintln!("  references built {built} in {load_time:?}");
    eprintln!("  load doors {}", doors.len());
    assert!(!doors.is_empty(), "no load door found in the cell");

    let (door, far_door) = doors[0];
    let player = game.world_mut().player();
    game.world_mut().move_to(player, door, [0.0; 3]);
    let before = game.world_mut().reference(player).unwrap().cell;

    let resident_before = game.world().loaded_reference_count();
    let started = Instant::now();
    assert!(game.activate(door, player), "the door activated");
    let move_time = started.elapsed();
    let resident_after = game.world().loaded_reference_count();
    eprintln!("  references resident {resident_before} to {resident_after}");

    let after = game.world_mut().reference(player).unwrap().cell;
    let resident = game.world().loaded_cell();
    eprintln!("  door {door:?} to {far_door:?}");
    eprintln!("  player cell {before:?} to {after:?} in {move_time:?}");
    eprintln!("  resident cell now {resident:?}");

    assert_ne!(before, after, "the player changed cell");
    assert_eq!(
        after, resident,
        "and the world is holding the cell it is in"
    );

    // A load door is a persistent child in the shipped files, which it has to
    // be: the cell on the far side names it in its own teleport data, so it
    // must resolve whether or not this cell is resident. So the door stays
    // after the unload, and that is correct rather than a leak. Measured
    // rather than assumed, because the first version of this test assumed the
    // opposite and was wrong.
    let persistent = game.world_mut().is_persistent_child(cell, door);
    eprintln!("  the door is a persistent child: {persistent}");
    assert!(
        persistent,
        "a load door that were temporary could not be named from the far side"
    );
    assert!(
        game.world().is_reference_loaded(door),
        "so it survives the unload"
    );
}
