// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_60_formlist_and_game_audit_inventory_is_registered() {
    let game = Game::new(LoadOrder::new());
    for name in ["GetSize", "GetAt", "HasForm", "Find", "AddForm"] {
        assert!(
            game.vm.natives.has_method("FormList", name),
            "FormList.{name}"
        );
    }
    for name in [
        "GetPlayer",
        "GetForm",
        "GetFormEx",
        "GetFormFromFile",
        "EnablePlayerControls",
        "DisablePlayerControls",
        "GetGameSettingFloat",
        "GetGameSettingInt",
        "GetGameSettingString",
        "SetGameSettingFloat",
        "SetGameSettingInt",
        "SetGameSettingBool",
        "SetGameSettingString",
        "GetRealHoursPassed",
        "IncrementStat",
        "QueryStat",
        "IsFastTravelEnabled",
        "EnableFastTravel",
        "SetPlayerAIDriven",
        "SetInChargen",
        "SetBeastForm",
        "TeachWord",
        "UnlockWord",
        "IsWordUnlocked",
        "AddAchievement",
        "GetPlayersLastRiddenHorse",
        "AddPerkPoints",
        "GetDialogueTarget",
    ] {
        assert!(game.vm.natives.has_global("Game", name), "Game.{name}");
    }
}
