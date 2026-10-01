// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_62_global_variable_and_owner_bindings_are_methods() {
    let game = Game::new(LoadOrder::new());
    for class in ["Alias", "Scene", "Package", "TopicInfo"] {
        assert!(
            game.vm.natives.has_method(class, "GetOwningQuest"),
            "{class}.GetOwningQuest"
        );
    }
    for name in ["GetValue", "GetValueInt", "SetValue", "SetValueInt", "Mod"] {
        assert!(
            game.vm.natives.has_method("GlobalVariable", name),
            "GlobalVariable.{name}"
        );
    }
    assert!(!game.vm.natives.has_global("GlobalVariable", "Mod"));
}
