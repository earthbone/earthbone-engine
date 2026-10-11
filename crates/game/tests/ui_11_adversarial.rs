// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::GlobalFormId;
use soe_papyrus::Value;

use crate::support::{ui_05_fixture as compiled, ui_11_fixture};

fn value(game: &mut soe_game::Game, name: &str) -> Value {
    let object = game.object_of_form(GlobalFormId {
        file: 0,
        object: compiled::CONFIG,
    });
    game.vm.get_var(object, compiled::CHILD, name).unwrap()
}

#[test]
fn ui_11_page_ids_reject_stale_page_selections() {
    let Some(fixture) = ui_11_fixture::load() else {
        return;
    };
    let mut game = fixture.game();
    let form = GlobalFormId {
        file: 0,
        object: compiled::CONFIG,
    };

    assert!(game.mcm_open(form));
    assert!(game.mcm_select_page(1));
    assert_eq!(game.mcm_page_number(), Some(2));
    assert_eq!(
        game.mcm_options()
            .iter()
            .map(|option| option.index)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
    assert!(!game.mcm_select_option(form, 0x100));
    assert!(game.mcm_select_option(form, 0x200));
    assert_eq!(value(&mut game, "selection_count"), Value::Int(1));
    assert_eq!(value(&mut game, "selected_option"), Value::Int(0x200));
    assert!(!game.mcm_select_option(form, 0x200));
    assert_eq!(value(&mut game, "selection_count"), Value::Int(1));
}

#[test]
fn ui_11_config_registration_and_callbacks_are_idempotent_per_open() {
    let Some(fixture) = ui_11_fixture::load() else {
        return;
    };
    let mut game = fixture.game();
    let form = GlobalFormId {
        file: 0,
        object: compiled::CONFIG,
    };

    for _ in 0..3 {
        assert_eq!(game.mcm_configs().len(), 1);
    }
    assert_eq!(value(&mut game, "register_count"), Value::Int(1));
    assert!(game.mcm_open(form));
    assert_eq!(value(&mut game, "open_count"), Value::Int(1));
    assert!(game.mcm_select_page(0));
    assert!(game.mcm_select_option(form, 0x100));
    assert_eq!(value(&mut game, "selection_count"), Value::Int(1));
    assert_eq!(value(&mut game, "selected_option"), Value::Int(0x100));
    let options = game.mcm_options();
    assert_eq!(options[0].flags & 1, 1);
    assert_eq!(options[0].string_value, "Selected");
    assert_eq!(options[1].flags & 1, 0);
    assert_eq!(options[1].number_value, 0.0);
}

#[test]
fn ui_11_selection_rejects_missing_form_and_out_of_range_id() {
    let Some(fixture) = ui_11_fixture::load() else {
        return;
    };
    let mut game = fixture.game();
    let form = GlobalFormId {
        file: 0,
        object: compiled::CONFIG,
    };

    assert!(!game.mcm_select_option(form, 0x100));
    assert!(game.mcm_open(form));
    assert!(!game.mcm_select_option(
        GlobalFormId {
            file: 0,
            object: u32::MAX,
        },
        0x100,
    ));
    assert!(!game.mcm_select_option(form, u32::MAX));
    assert_eq!(value(&mut game, "selection_count"), Value::Int(0));
}
