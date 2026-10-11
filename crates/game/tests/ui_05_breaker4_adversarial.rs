// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/ui_05_fixture.rs"]
mod compiled;

use soe_esm::GlobalFormId;
use soe_papyrus::Value;

#[test]
fn ui_05_real_manager_reopens_and_closes_the_active_config_once() {
    let Some(fixture) = compiled::load() else {
        eprintln!("UI-05 adversarial manager check skipped: SkyUI PEX input is unset");
        return;
    };
    let mut game = fixture.game();
    let config = GlobalFormId {
        file: 0,
        object: compiled::CONFIG,
    };
    let object = game.object_of_form(config);
    let get =
        |game: &soe_game::Game, name: &str| game.vm.get_var(object, compiled::CHILD, name).unwrap();

    game.mcm_begin();
    game.mcm_begin();
    assert!(game.mcm_open(config));
    assert!(game.mcm_select_page(1));
    assert_eq!(get(&game, "last_page"), Value::string("Advanced"));

    assert!(game.mcm_open(config));
    assert_eq!(get(&game, "open_count"), Value::Int(2));
    assert_eq!(get(&game, "close_count"), Value::Int(1));
    assert_eq!(get(&game, "last_page"), Value::string(""));

    game.mcm_close();
    game.mcm_close();
    assert_eq!(get(&game, "close_count"), Value::Int(2));
    game.mcm_begin();
    assert!(game.mcm_open(config));
    assert!(game.mcm_select_page(0));
    assert_eq!(get(&game, "open_count"), Value::Int(3));
    assert_eq!(get(&game, "register_count"), Value::Int(1));
    assert_eq!(get(&game, "init_count"), Value::Int(1));
    assert_eq!(get(&game, "last_page"), Value::string("General"));
}
