// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_61_only_audited_getid_getname_bindings_are_present() {
    let game = Game::new(LoadOrder::new());
    for (class, name) in [
        ("Form", "GetName"),
        ("Quest", "GetID"),
        ("Alias", "GetID"),
        ("Alias", "GetName"),
    ] {
        assert!(game.vm.natives.has_method(class, name), "{class}.{name}");
    }
    assert!(!game.vm.natives.has_global("Form", "GetName"));
    assert!(!game.vm.natives.has_global("Quest", "GetID"));
}
