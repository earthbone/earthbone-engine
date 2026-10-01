// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ObjectReference.Is3DLoaded follows host cell resource residency.

#[path = "transitions.rs"]
#[allow(dead_code)]
mod transitions;

use transitions::{game, id};

const CELL_B: u32 = 0x811;
const ROCK_B: u32 = 0x840;

#[test]
fn hst_13_reference_3d_tracks_cell_streaming_and_enabled_state() {
    let game = game();
    let cell = id(CELL_B);
    let reference = id(ROCK_B);

    // Building an unrelated reference does not report host 3D residency.
    game.world_mut().reference(reference).unwrap();
    assert!(!game.world().is_reference_3d_loaded(reference));

    // This is the headless host streaming fixture: cell attachment publishes
    // a resource arrival to the world, which now answers true for its ref.
    game.world_mut().attach_streaming_cells(&[cell]).unwrap();
    assert!(game.world().is_reference_3d_loaded(reference));

    game.world_mut().reference(reference).unwrap().enabled = false;
    assert!(!game.world().is_reference_3d_loaded(reference));
    game.world_mut().reference(reference).unwrap().enabled = true;
    assert!(game.world().is_reference_3d_loaded(reference));
    game.world_mut().reference(reference).unwrap().position[0] += 1.0;

    // The query turns false when the host releases the owning cell, even
    // though the runtime may retain a changed reference in memory.
    game.world_mut().detach_streaming_cells(&[cell]).unwrap();
    assert!(game.world().is_reference_loaded(reference));
    assert!(!game.world().is_reference_3d_loaded(reference));
}
