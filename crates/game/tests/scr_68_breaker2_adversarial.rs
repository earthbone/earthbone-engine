// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic, clippy::unwrap_used)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_68_quest_transition_queries_are_registered_as_methods() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    assert!(game.vm.natives.has_method("Quest", "IsStarting"));
    assert!(game.vm.natives.has_method("Quest", "IsStopping"));
}
