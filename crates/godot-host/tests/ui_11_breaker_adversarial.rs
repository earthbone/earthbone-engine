// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::mcm_menu::ConfigPage;

#[test]
fn ui_11_missing_script_form_does_not_report_a_selection_callback() {
    let mut game = Game::new(LoadOrder::new());
    let mut page = ConfigPage::new(1);
    let option = page.add_text("Status", "Ready", 0).unwrap();
    let missing_script = GlobalFormId {
        file: 0,
        object: 0x800,
    };

    assert!(!page.select(&mut game, missing_script, option));
}

#[test]
fn ui_11_selection_stays_blocked_when_hidden_option_is_enabled() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(soe_game::quest::build_record(
        0x800,
        "McmFixture",
        soe_game::quest::flags::START_GAME_ENABLED,
    ));
    let mut order = LoadOrder::new();
    order.add("McmFixture.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let mut page = ConfigPage::new(1);
    let option = page
        .add_text("Status", "Ready", soe_godot_host::mcm_menu::OPTION_FLAG_DISABLED | 0x02)
        .unwrap();

    assert!(page.set_enabled(option, true));
    assert!(!page.rows()[0].visible());
    assert!(!page.select(
        &mut game,
        GlobalFormId {
            file: 0,
            object: 0x800,
        },
        option,
    ));
}
