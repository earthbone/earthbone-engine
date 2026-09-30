// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OPTION_FLAG_DISABLED};
use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, IAdd};
use soe_papyrus::Value;

#[test]
fn ui_11_option_ids_stay_within_signed_papyrus_integer_range() {
    let mut page = ConfigPage::new(0x80_0000);

    assert_eq!(page.add_text("Too far", "", 0), None);
}

#[test]
fn ui_11_enabled_selection_delivers_one_callback_with_the_skyui_id() {
    let mut order = LoadOrder::new();
    order.add("Fixture.esp", Plugin::new(FileHeader::default())).unwrap();
    let mut game = Game::new(order);
    let form = GlobalFormId { file: 0, object: 0x123 };
    let function = FunctionBuilder::new("OnOptionSelect", "None")
        .param("option", "Int")
        .ins(Assign, vec![ident("last"), ident("option")])
        .ins(IAdd, vec![ident("calls"), ident("calls"), i(1)])
        .build();
    let script = ScriptBuilder::new("McmObserver", "Quest")
        .variable("last", "Int", i(-1))
        .variable("calls", "Int", i(0))
        .function(function);
    game.load_script(&script.bytes()).unwrap();
    let object = game.object_of_form(form);
    game.vm.attach(object, "McmObserver").unwrap();

    let mut page = ConfigPage::new(1);
    let option = page.add_text("Status", "Ready", 0).unwrap();
    let disabled = page
        .add_toggle("Disabled", false, OPTION_FLAG_DISABLED)
        .unwrap();
    assert!(page.select(&mut game, form, option));
    assert!(!page.select(&mut game, form, disabled));

    assert_eq!(game.vm.get_var(object, "McmObserver", "calls"), Some(Value::Int(1)));
    assert_eq!(game.vm.get_var(object, "McmObserver", "last"), Some(Value::Int(0x100)));
}
