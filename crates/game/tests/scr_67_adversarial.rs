// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_67_audit_inventory_is_not_missing_documented_high_risk_bindings() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    for name in ["MoveTo", "RemoveAllItems", "RemoveItem", "Activate"] {
        assert!(game.vm.natives.has_method("ObjectReference", name));
        assert!(!game.vm.natives.has_global("ObjectReference", name));
    }
    for name in ["Add", "Remove"] {
        assert!(game.vm.natives.has_method("MusicType", name));
    }
}
