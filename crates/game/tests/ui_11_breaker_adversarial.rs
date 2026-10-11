// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/ui_05_fixture.rs"]
mod ui_05_fixture;
#[path = "support/ui_11_fixture.rs"]
mod ui_11_fixture;

use soe_esm::GlobalFormId;
use soe_papyrus::Value;

fn fixture() -> Option<ui_05_fixture::Fixture> {
    if std::env::var_os("SOE_TEST_SKYUI").is_none()
        && std::env::var_os("SOE_SKYUI_CONFIG_BASE_PEX").is_none()
    {
        return None;
    }
    ui_11_fixture::load()
}

fn state_fixture() -> Option<ui_05_fixture::Fixture> {
    if std::env::var_os("SOE_TEST_SKYUI").is_none()
        && std::env::var_os("SOE_SKYUI_CONFIG_BASE_PEX").is_none()
    {
        return None;
    }
    ui_11_fixture::load_state_options()
}

const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: ui_05_fixture::CONFIG,
};

fn child_var(game: &mut soe_game::Game, name: &str) -> Value {
    let object = game.object_of_form(FORM);
    game.vm.get_var(object, ui_05_fixture::CHILD, name).unwrap()
}

#[test]
fn ui_11_breaker_preserves_sparse_option_positions_and_flag_bits() {
    let Some(fixture) = fixture() else {
        return;
    };
    let mut game = fixture.game();
    assert!(game.mcm_open(FORM));
    assert!(game.mcm_select_page(0));

    let options = game.mcm_options();
    assert_eq!(options.len(), 5);
    assert_eq!(options[4].index, 4);
    assert_eq!(options[4].option_type, 2);
    assert_eq!(options[4].flags, 2);
    assert!(!game.mcm_select_option(FORM, 0x104));
    assert!(!game.mcm_select_option(FORM, 0x101));
    assert_eq!(child_var(&mut game, "selection_count"), Value::Int(0));
}

#[test]
fn ui_11_breaker_allows_unmap_flag_and_dispatches_each_press_once() {
    let Some(fixture) = fixture() else {
        return;
    };
    let mut game = fixture.game();
    assert!(game.mcm_open(FORM));
    assert!(game.mcm_select_page(0));
    // OPTION_FLAG_WITH_UNMAP is independent of DISABLED and HIDDEN.
    let object = game.object_of_form(FORM);
    let thread = game
        .vm
        .call(
            object,
            "SetOptionFlags",
            &[Value::Int(0x100), Value::Int(4), Value::Bool(false)],
        )
        .unwrap();
    game.run_until_idle(64);
    assert!(game.vm.thread_result(thread).is_some());
    assert_eq!(game.mcm_options()[0].flags, 4);
    assert!(game.mcm_select_option(FORM, 0x100));
    assert!(!game.mcm_select_option(FORM, 0x100));
    assert_eq!(child_var(&mut game, "selection_count"), Value::Int(1));
}

#[test]
fn ui_11_breaker_state_callbacks_update_value_and_disable_the_row() {
    let Some(fixture) = state_fixture() else {
        return;
    };
    let mut game = fixture.game();
    assert!(game.mcm_open(FORM));
    assert!(game.mcm_select_page(0));

    // The source sends state options to OnSelectST in the matching script state.
    assert!(game.mcm_select_option(FORM, 0x100));
    assert_eq!(child_var(&mut game, "text_state_count"), Value::Int(1));
    assert_eq!(game.mcm_options()[0].string_value, "Selected");
    assert_eq!(game.mcm_options()[0].flags, 1);
    assert!(!game.mcm_select_option(FORM, 0x100));
    assert_eq!(child_var(&mut game, "text_state_count"), Value::Int(1));

    assert!(game.mcm_select_option(FORM, 0x102));
    assert_eq!(child_var(&mut game, "toggle_state_count"), Value::Int(1));
    assert_eq!(game.mcm_options()[2].number_value, 0.0);
    assert_eq!(game.mcm_options()[2].flags, 1);
    assert!(!game.mcm_select_option(FORM, 0x102));
    assert_eq!(child_var(&mut game, "toggle_state_count"), Value::Int(1));
}

#[test]
fn ui_11_breaker_page_local_slots_keep_distinct_callback_ids() {
    let Some(fixture) = fixture() else {
        return;
    };
    let mut game = fixture.game();
    assert!(game.mcm_open(FORM));
    assert!(game.mcm_select_page(0));
    assert!(game.mcm_select_option(FORM, 0x100));
    assert_eq!(child_var(&mut game, "selected_option"), Value::Int(0x100));

    assert!(game.mcm_select_page(1));
    assert_eq!(game.mcm_page_number(), Some(2));
    assert!(!game.mcm_select_option(FORM, 0x100));
    assert!(game.mcm_select_option(FORM, 0x200));
    assert_eq!(child_var(&mut game, "selected_option"), Value::Int(0x200));
    assert_eq!(child_var(&mut game, "selection_count"), Value::Int(2));
}
