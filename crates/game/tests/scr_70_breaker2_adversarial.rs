// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_70_sound_registry_includes_every_shipped_vanilla_method() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    for method in ["Play", "PlayAndWait"] {
        assert!(
            game.vm.natives.has_method("Sound", method),
            "Sound.psc declares Sound.{method}"
        );
    }
}
