// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_68_alias_clear_binding_is_instance_method_and_quest_getquest_is_global() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    assert!(game.vm.natives.has_method("Alias", "Clear"));
    assert!(!game.vm.natives.has_global("Alias", "Clear"));
    assert!(game.vm.natives.has_global("Quest", "GetQuest"));
    assert!(!game.vm.natives.has_method("Quest", "GetQuest"));
}
