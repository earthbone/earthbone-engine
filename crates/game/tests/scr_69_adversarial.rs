// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_69_scene_and_scroll_contract_bindings_keep_their_registry_shape() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    for (class, name) in [
        ("Scene", "IsActionComplete"),
        ("Scene", "IsPlaying"),
        ("Scroll", "Cast"),
    ] {
        assert!(game.vm.natives.has_method(class, name), "{class}.{name}");
        assert!(!game.vm.natives.has_global(class, name), "{class}.{name}");
    }
    assert!(!game.vm.natives.has_method("Scroll", "Play"));
}
