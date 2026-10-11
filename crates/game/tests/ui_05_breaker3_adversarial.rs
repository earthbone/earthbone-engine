// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/ui_05_fixture.rs"]
mod compiled;

use soe_esm::GlobalFormId;
use soe_papyrus::Value;

#[test]
fn ui_05_reselecting_config_closes_then_reinitializes_its_page() {
    let Some(fixture) = compiled::load() else {
        eprintln!("UI-05 adversarial PEX check skipped: SkyUI PEX input is unset");
        return;
    };
    let mut game = fixture.game();
    let form = GlobalFormId {
        file: 0,
        object: compiled::CONFIG,
    };
    let object = game.object_of_form(form);
    let value = |game: &soe_game::Game, name: &str| {
        game.vm.get_var(object, compiled::CHILD, name).unwrap()
    };

    assert!(game.mcm_open(form));
    assert!(game.mcm_select_page(1));
    assert_eq!(value(&game, "last_page"), Value::string("Advanced"));

    assert!(game.mcm_open(form));
    assert_eq!(value(&game, "close_count"), Value::Int(1));
    assert_eq!(value(&game, "open_count"), Value::Int(2));
    assert_eq!(value(&game, "reset_count"), Value::Int(3));
    assert_eq!(value(&game, "last_page"), Value::string(""));

    game.mcm_close();
    game.mcm_close();
    assert_eq!(value(&game, "close_count"), Value::Int(2));
}
