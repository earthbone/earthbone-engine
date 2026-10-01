// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-12 head-tracking state round-trip regression.

use soe_esm::{GlobalFormId, LoadOrder};
use soe_game::Game;

#[test]
fn scr_12_unset_and_explicit_head_tracking_survive_save_load() {
    let mut game = Game::new(LoadOrder::new());
    let actor = GlobalFormId {
        file: 0,
        object: 0x14,
    };
    assert!(game.world_mut().set_head_tracking(actor, false).is_err());
    let saved = game.world_mut().save_state();
    let loaded = Game::new(LoadOrder::new());
    loaded.world_mut().load_state(&saved).unwrap();
    assert_eq!(loaded.world_mut().head_tracking(actor), None);
}
