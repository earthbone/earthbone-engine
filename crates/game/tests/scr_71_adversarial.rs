// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_71_spell_and_topic_bindings_are_instance_methods() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    for (class, name) in [("Spell", "Cast"), ("Spell", "IsHostile"), ("Spell", "RemoteCast"), ("Spell", "Preload"), ("Spell", "Unload"), ("Topic", "Add")] {
        assert!(game.vm.natives.has_method(class, name), "{class}.{name}");
        assert!(!game.vm.natives.has_global(class, name), "{class}.{name}");
    }
    assert!(!game.vm.natives.has_method("Topic", "Remove"));
}
