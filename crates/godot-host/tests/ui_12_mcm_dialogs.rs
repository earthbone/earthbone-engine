#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OptionKind};
use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

const CONFIG_SCRIPT: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x123,
};

fn game() -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Fixture.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let script = ScriptBuilder::new("Ui12Config", "")
        .variable("::slider_open", "Int", i(-1))
        .variable("::slider_accept", "Float", i(0))
        .variable("::menu_open", "Int", i(-1))
        .variable("::menu_accept", "Int", i(-1))
        .function(
            FunctionBuilder::new("OnOptionSliderOpen", "None")
                .param("option", "Int")
                .ins(Assign, vec![ident("::slider_open"), ident("option")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnOptionSliderAccept", "None")
                .param("option", "Int")
                .param("value", "Float")
                .ins(Assign, vec![ident("::slider_accept"), ident("value")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnOptionMenuOpen", "None")
                .param("option", "Int")
                .ins(Assign, vec![ident("::menu_open"), ident("option")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnOptionMenuAccept", "None")
                .param("option", "Int")
                .param("index", "Int")
                .ins(Assign, vec![ident("::menu_accept"), ident("index")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let object = game.object_of_form(CONFIG_SCRIPT);
    game.vm.attach(object, "Ui12Config").unwrap();
    game
}

#[test]
fn ui_12_slider_open_dispatches_callback_with_authored_bounds() {
    let mut game = game();
    let mut page = ConfigPage::new(1);
    let slider = page.add_slider("Scale", 1.5, 0.5, 3.0, 0.5, 0).unwrap();

    let dialog = page.open_slider(&mut game, CONFIG_SCRIPT, slider).unwrap();

    assert_eq!(dialog.option_id, slider);
    assert_eq!(dialog.start_value, 1.5);
    assert_eq!(dialog.default_value, 1.5);
    assert_eq!(dialog.min_value, 0.5);
    assert_eq!(dialog.max_value, 3.0);
    assert_eq!(dialog.interval, 0.5);
    let object = game.object_of_form(CONFIG_SCRIPT);
    assert_eq!(
        game.vm.get_var(object, "Ui12Config", "::slider_open"),
        Some(Value::Int(slider as i32))
    );
}

#[test]
fn ui_12_slider_accept_dispatches_selected_value_within_authored_bounds() {
    let mut game = game();
    let mut page = ConfigPage::new(0);
    let slider = page.add_slider("Scale", 1.0, 0.5, 3.0, 0.5, 0).unwrap();

    assert!(page.open_slider(&mut game, CONFIG_SCRIPT, slider).is_some());
    assert!(page.accept_slider(&mut game, CONFIG_SCRIPT, slider, 4.0));

    assert_eq!(
        page.rows()[0].kind,
        OptionKind::Slider {
            value: 3.0,
            min_value: 0.5,
            max_value: 3.0,
            interval: 0.5,
        }
    );
    let object = game.object_of_form(CONFIG_SCRIPT);
    assert_eq!(
        game.vm
            .get_var(object, "Ui12Config", "::slider_accept")
            .unwrap()
            .as_float(),
        3.0
    );
}

#[test]
fn ui_12_slider_accept_snaps_to_authored_interval() {
    let mut game = game();
    let mut page = ConfigPage::new(0);
    let slider = page.add_slider("Scale", 1.0, 0.5, 3.0, 0.5, 0).unwrap();

    assert!(page.open_slider(&mut game, CONFIG_SCRIPT, slider).is_some());
    assert!(page.accept_slider(&mut game, CONFIG_SCRIPT, slider, 2.6));

    assert!(matches!(
        page.rows()[0].kind,
        OptionKind::Slider { value: 2.5, .. }
    ));
    let object = game.object_of_form(CONFIG_SCRIPT);
    assert_eq!(
        game.vm
            .get_var(object, "Ui12Config", "::slider_accept")
            .unwrap()
            .as_float(),
        2.5
    );
}

#[test]
fn ui_12_menu_open_and_accept_dispatch_selected_choice_index() {
    let mut game = game();
    let mut page = ConfigPage::new(2);
    let menu = page
        .add_menu(
            "Mode",
            "Balanced",
            vec!["Quiet".into(), "Balanced".into(), "Fast".into()],
            0,
        )
        .unwrap();

    let dialog = page.open_menu(&mut game, CONFIG_SCRIPT, menu).unwrap();
    assert_eq!(dialog.option_id, menu);
    assert_eq!(dialog.start_index, 1);
    assert_eq!(dialog.default_index, 1);
    assert_eq!(dialog.options, ["Quiet", "Balanced", "Fast"]);
    assert!(page.accept_menu(&mut game, CONFIG_SCRIPT, menu, 2));

    assert!(matches!(
        &page.rows()[0].kind,
        OptionKind::Menu { value, selected_index: 2, .. } if value == "Fast"
    ));
    let object = game.object_of_form(CONFIG_SCRIPT);
    assert_eq!(
        game.vm.get_var(object, "Ui12Config", "::menu_open"),
        Some(Value::Int(menu as i32))
    );
    assert_eq!(
        game.vm.get_var(object, "Ui12Config", "::menu_accept"),
        Some(Value::Int(2))
    );
}

#[test]
fn ui_12_cancel_keeps_selected_values_and_sends_no_accept_callback() {
    let mut game = game();
    let mut page = ConfigPage::new(0);
    let slider = page.add_slider("Scale", 1.5, 0.5, 3.0, 0.5, 0).unwrap();
    let menu = page
        .add_menu(
            "Mode",
            "Balanced",
            vec!["Balanced".into(), "Fast".into()],
            0,
        )
        .unwrap();

    assert!(page.open_slider(&mut game, CONFIG_SCRIPT, slider).is_some());
    assert!(page.cancel_dialog(slider));
    assert!(page.open_menu(&mut game, CONFIG_SCRIPT, menu).is_some());
    assert!(page.cancel_dialog(menu));
    assert_eq!(
        page.rows()[0].kind,
        OptionKind::Slider {
            value: 1.5,
            min_value: 0.5,
            max_value: 3.0,
            interval: 0.5,
        }
    );
    assert!(matches!(
        &page.rows()[1].kind,
        OptionKind::Menu { value, selected_index: 0, .. } if value == "Balanced"
    ));
    let object = game.object_of_form(CONFIG_SCRIPT);
    assert_eq!(
        game.vm.get_var(object, "Ui12Config", "::slider_open"),
        Some(Value::Int(slider as i32))
    );
    assert_eq!(
        game.vm.get_var(object, "Ui12Config", "::menu_open"),
        Some(Value::Int(menu as i32))
    );
    assert_eq!(
        game.vm
            .get_var(object, "Ui12Config", "::slider_accept")
            .unwrap()
            .as_float(),
        0.0
    );
    assert_eq!(
        game.vm.get_var(object, "Ui12Config", "::menu_accept"),
        Some(Value::Int(-1))
    );
}

#[test]
fn ui_12_rejects_invalid_authored_slider_ranges_and_menu_choices() {
    let mut page = ConfigPage::new(0);

    assert_eq!(page.add_slider("Bad", 4.0, 0.0, 3.0, 1.0, 0), None);
    assert_eq!(page.add_slider("Bad", 1.0, 0.0, 3.0, 0.0, 0), None);
    assert_eq!(
        page.add_menu("Bad", "Missing", vec!["Known".into()], 0),
        None
    );
}
