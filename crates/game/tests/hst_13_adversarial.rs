// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "transitions.rs"]
#[allow(dead_code)]
mod transitions;

use transitions::{game, id};

const CELL_B: u32 = 0x811;
const ROCK_B: u32 = 0x840;

#[test]
fn hst_13_prefetch_does_not_claim_host_3d_residency() {
    let game = game();
    let reference = id(ROCK_B);
    game.world_mut().prefetch_cell(id(CELL_B));

    assert!(!game.world().is_reference_3d_loaded(reference));
}
