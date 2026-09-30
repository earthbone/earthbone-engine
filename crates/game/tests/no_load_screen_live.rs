// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! What the split actually costs on the installed game. Gated on
//! `SKYRIM_DATA`, skips cleanly without it.
//!
//! The bar is set from the design rather than from the measurement, which is
//! the only way a threshold means anything. A door plays an open animation of
//! roughly half a second. The prefetch has that whole animation to finish in.
//! The commit is the one piece the player waits on at the moment of crossing,
//! so it has to fit inside a frame: 16.7 ms at 60 Hz, and this asks for 5 ms
//! to leave room for everything else happening that frame.
//!
//! What this does not cover: textures and meshes, which live in
//! `crates/godot-host` and are the half a cold cache actually punishes. These
//! numbers are the record and reference side alone.

#[path = "common/installed.rs"]
mod installed;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use soe_esm::GlobalFormId;
use soe_game::Game;

const MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

/// Interiors to try, largest last. Any that a load order does not carry are
/// skipped rather than failed, so this stays honest on a partial install.
const INTERIORS: [&str; 6] = [
    "WhiterunBanneredMare",
    "WhiterunJorrvaskr",
    "RiftenBeeandBarb",
    "WhiterunDragonsreach",
    "SolitudeBluePalace",
    "MarkarthUnderstoneKeep",
];

/// The commit is the only part the player waits on. One frame is 16.7 ms.
const COMMIT_BUDGET: Duration = Duration::from_millis(5);

/// What the door animation gives the prefetch, at the short end.
const ANIMATION_BUDGET: Duration = Duration::from_millis(500);

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

fn real_game(_data: &std::path::Path) -> Game {
    let order = installed::load_order(&MASTERS).unwrap();
    Game::from_shared_order(order)
}

/// The first load door in a cell, with the cell it leads to.
fn first_load_door(g: &mut Game, cell: GlobalFormId) -> Option<(GlobalFormId, GlobalFormId)> {
    let doors = g.world_mut().cell_doors(cell);
    for (door, far) in doors {
        let far = far?;
        if let Some(target) = g.world_mut().reference(far).and_then(|r| r.cell) {
            return Some((door, target));
        }
    }
    None
}

#[test]
fn the_split_moves_the_cost_off_the_crossing() {
    let Some(data) = data_dir() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };

    let mut measured = 0;
    let mut worst_commit = Duration::ZERO;
    let mut worst_prefetch = Duration::ZERO;
    let mut worst_cell = String::new();

    eprintln!(
        "{:<26} {:>6} {:>10} {:>10} {:>10} {:>10}",
        "interior", "refs", "cold move", "prefetch", "commit", "settle"
    );
    for name in INTERIORS {
        // A fresh game per interior, so every prefetch measured is genuinely
        // cold rather than warmed by the run before it.
        let mut g = real_game(&data);
        let Some(cell) = g.world_mut().cell_by_editor_id(name) else {
            eprintln!("{name:<26} not in this load order, skipped");
            continue;
        };
        g.world_mut().load_cell(cell);
        let Some((door, target)) = first_load_door(&mut g, cell) else {
            eprintln!("{name:<26} no load door, skipped");
            continue;
        };

        // The old path: one call that builds and attaches together.
        let cold = real_game(&data);
        cold.world_mut().load_cell(cell);
        let started = Instant::now();
        let moved = cold.world_mut().change_cell(Some(target));
        let cold_move = started.elapsed();

        // The split path, on a second game so the destination is still cold.
        let started = Instant::now();
        let way = g.world_mut().arm_doorway(door).expect("the door teleports");
        let prefetch = started.elapsed();
        let started = Instant::now();
        let committed = g.world_mut().commit_doorway(&way);
        let commit = started.elapsed();
        // The part that moved off the crossing, measured so it is not free by
        // assertion.
        let started = Instant::now();
        let (dropped, kept) = g.world_mut().settle_unload();
        let settle = started.elapsed();
        assert!(
            dropped + kept > 0,
            "{name}: nothing was waiting to be dropped, so the defer did nothing"
        );

        eprintln!(
            "{name:<26} {:>6} {cold_move:>10.2?} {prefetch:>10.2?} {commit:>10.2?} {settle:>10.2?}",
            moved.loaded
        );

        assert_eq!(
            committed.loaded, 0,
            "{name}: the commit still built {} references, so the prefetch missed some",
            committed.loaded
        );
        assert!(
            commit < COMMIT_BUDGET,
            "{name}: the crossing cost {commit:?}, over the {COMMIT_BUDGET:?} frame budget"
        );
        assert!(
            prefetch < ANIMATION_BUDGET,
            "{name}: the prefetch cost {prefetch:?}, over the {ANIMATION_BUDGET:?} the door animation gives it"
        );

        if commit > worst_commit {
            worst_commit = commit;
            worst_cell = name.to_string();
        }
        worst_prefetch = worst_prefetch.max(prefetch);
        measured += 1;
    }

    assert!(
        measured > 0,
        "no interior in the list resolved, nothing measured"
    );
    eprintln!("interiors measured {measured}");
    eprintln!("worst prefetch {worst_prefetch:?}, inside the {ANIMATION_BUDGET:?} animation");
    eprintln!("worst commit {worst_commit:?} ({worst_cell}), inside the {COMMIT_BUDGET:?} frame");
}

#[test]
fn the_real_event_sequence_does_not_change_when_the_far_side_was_streamed() {
    let Some(data) = data_dir() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };

    let names = |g: &Game| -> Vec<String> {
        g.world()
            .pending
            .iter()
            .filter_map(|p| match p {
                soe_game::Pending::Event { object, name, .. } => Some(format!("{object:?} {name}")),
                soe_game::Pending::Call { .. }
                | soe_game::Pending::CallScript { .. }
                | soe_game::Pending::BindEffect { .. }
                | soe_game::Pending::DeferredDeath { .. }
                | soe_game::Pending::ActorDeath { .. } => None,
            })
            .collect()
    };

    let setup = |g: &mut Game| -> (GlobalFormId, GlobalFormId) {
        let cell = g
            .world_mut()
            .cell_by_editor_id("WhiterunBanneredMare")
            .expect("the Bannered Mare is in Skyrim.esm");
        g.world_mut().load_cell(cell);
        let pair = first_load_door(g, cell).expect("the tavern has a door onto the street");
        g.world_mut().pending.clear();
        pair
    };

    // Bind first on both, so the same references carry the same objects and
    // the two event lists are comparable at all.
    let mut cold = real_game(&data);
    cold.bind();
    let (_, target) = setup(&mut cold);
    cold.world_mut().change_cell(Some(target));
    let from_cold = names(&cold);

    let mut warm = real_game(&data);
    warm.bind();
    let (door, _) = setup(&mut warm);
    let way = warm
        .world_mut()
        .arm_doorway(door)
        .expect("the door teleports");
    warm.world_mut().commit_doorway(&way);
    let from_warm = names(&warm);

    eprintln!("events on the cold move {}", from_cold.len());
    eprintln!("events on the streamed move {}", from_warm.len());
    assert!(
        !from_cold.is_empty(),
        "the real move queued no events, so this compares nothing"
    );
    assert_eq!(
        from_cold, from_warm,
        "streaming the far side changed what scripts in Whiterun receive"
    );
}
