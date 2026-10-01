// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_64_actor_and_item_count_contract_does_not_leak_to_spells() {
    let game = Game::new(LoadOrder::new());
    for class in ["LeveledActor", "LeveledItem"] {
        for name in ["GetNthCount", "SetNthCount", "SetNthLevel"] {
            assert!(game.vm.natives.has_method(class, name), "{class}.{name}");
        }
    }
    assert!(!game.vm.natives.has_method("LeveledSpell", "GetNthCount"));
    assert!(!game.vm.natives.has_method("LeveledSpell", "SetNthCount"));
}
