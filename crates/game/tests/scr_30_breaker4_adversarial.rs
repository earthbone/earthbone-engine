// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_30_precache_and_training_bindings_are_registered() {
    let game = Game::new(LoadOrder::new());

    for name in [
        "PrecacheCharGen",
        "PrecacheCharGenClear",
        "ShowTrainingMenu",
        "ShowFirstPersonGeometry",
        "SetCameraTarget",
    ] {
        assert!(game.vm.natives.has_global("Game", name), "Game.{name}");
    }
}
