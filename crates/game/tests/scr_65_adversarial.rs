// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_65_leveled_spell_and_location_registry_matches_local_scripts() {
    let game = Game::new(LoadOrder::new());
    for name in [
        "AddForm",
        "Revert",
        "GetNumForms",
        "GetNthForm",
        "GetNthLevel",
        "SetNthLevel",
        "GetChanceNone",
        "SetChanceNone",
    ] {
        assert!(
            game.vm.natives.has_method("LeveledSpell", name),
            "LeveledSpell.{name}"
        );
    }
    for name in [
        "GetKeywordData",
        "SetKeywordData",
        "IsCleared",
        "SetCleared",
        "IsChild",
        "HasCommonParent",
        "IsSameLocation",
    ] {
        assert!(
            game.vm.natives.has_method("Location", name),
            "Location.{name}"
        );
    }
}
