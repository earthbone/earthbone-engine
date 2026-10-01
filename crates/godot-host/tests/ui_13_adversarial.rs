// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, KeyConflictScanner, OptionKind};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{NativeReturn, Opcode, Value};

const SCRIPT: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x123,
};

fn game_with_probe() -> (Game, Rc<RefCell<Vec<Vec<Value>>>>) {
    let mut order = LoadOrder::new();
    order
        .add("Ui13Adversarial.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&calls);
    assert!(game.vm.natives.register_global(
        "Ui13AdversarialProbe",
        "Record",
        Box::new(move |call| {
            captured.borrow_mut().push(call.args.clone());
            NativeReturn::Value(Value::None)
        }),
    ));
    let key_callback = FunctionBuilder::new("OnOptionKeyMapChange", "None")
        .param("option", "Int")
        .param("keyCode", "Int")
        .param("conflictControl", "String")
        .param("conflictName", "String")
        .ins(
            Opcode::CallStatic,
            vec![
                s("Ui13AdversarialProbe"),
                s("Record"),
                ident("::nonevar"),
                ident("option"),
                ident("keyCode"),
                ident("conflictControl"),
                ident("conflictName"),
            ],
        )
        .build();
    let color_open = FunctionBuilder::new("OnOptionColorOpen", "None")
        .param("option", "Int")
        .ins(
            Opcode::CallStatic,
            vec![
                s("Ui13AdversarialProbe"),
                s("Record"),
                ident("::nonevar"),
                ident("option"),
            ],
        )
        .build();
    let color_accept = FunctionBuilder::new("OnOptionColorAccept", "None")
        .param("option", "Int")
        .param("color", "Int")
        .ins(
            Opcode::CallStatic,
            vec![
                s("Ui13AdversarialProbe"),
                s("Record"),
                ident("::nonevar"),
                ident("option"),
                ident("color"),
            ],
        )
        .build();
    game.load_script(
        &ScriptBuilder::new("Ui13AdversarialConfig", "")
            .function(key_callback)
            .function(color_open)
            .function(color_accept)
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(SCRIPT);
    game.vm.attach(object, "Ui13AdversarialConfig").unwrap();
    (game, calls)
}

#[test]
fn ui_13_empty_vanilla_and_custom_mappings_do_not_hide_later_conflicts() {
    let mut scanner = KeyConflictScanner::default();
    scanner.set_vanilla_control(44, "");
    let empty = scanner.register_mod_config("Empty Mod");
    let first = scanner.register_mod_config("First Mod");
    let later = scanner.register_mod_config("Later Mod");
    assert!(scanner.set_custom_control(empty, 44, ""));
    assert!(scanner.set_custom_control(first, 44, "Roll"));
    assert!(scanner.set_custom_control(later, 44, "Sprint"));

    let conflict = scanner.conflict(44);
    assert_eq!(conflict.control, "Roll");
    assert_eq!(conflict.name, "First Mod");
}

#[test]
fn ui_13_remapping_only_notifies_and_leaves_key_value_for_config_script() {
    let (mut game, calls) = game_with_probe();
    let mut page = ConfigPage::new(0);
    let id = page.add_keymap("Dodge", 42, 0).unwrap();
    let scanner = KeyConflictScanner::default();

    assert!(page.remap_key_scanned(&mut game, SCRIPT, id, 44, &scanner));
    assert_eq!(page.rows()[0].kind, OptionKind::KeyMap { key_code: 42 });
    assert_eq!(
        calls.borrow().as_slice(),
        &[vec![
            Value::Int(id as i32),
            Value::Int(44),
            Value::string(""),
            Value::string(""),
        ]]
    );
}

#[test]
fn ui_13_scanned_remap_rejects_hidden_disabled_and_wrong_option_without_callback() {
    let (mut game, calls) = game_with_probe();
    let mut page = ConfigPage::new(0);
    let hidden = page.add_keymap("Hidden", 42, 2).unwrap();
    let disabled = page.add_keymap("Disabled", 42, 1).unwrap();
    let text = page.add_text("Text", "value", 0).unwrap();
    let scanner = KeyConflictScanner::default();

    assert!(!page.remap_key_scanned(&mut game, SCRIPT, hidden, 44, &scanner));
    assert!(!page.remap_key_scanned(&mut game, SCRIPT, disabled, 44, &scanner));
    assert!(!page.remap_key_scanned(&mut game, SCRIPT, text, 44, &scanner));
    assert!(calls.borrow().is_empty());
}

#[test]
fn ui_13_color_cancel_and_mismatched_accept_preserve_value() {
    let (mut game, calls) = game_with_probe();
    let mut page = ConfigPage::new(0);
    let id = page.add_color("Tint", 0x123456, 0).unwrap();

    assert!(page.open_color_dialog(&mut game, SCRIPT, id));
    assert!(!page.accept_color(&mut game, SCRIPT, id + 1, 0xabcdef));
    assert_eq!(page.rows()[0].kind, OptionKind::Color { value: 0x123456 });
    assert!(page.cancel_color_dialog(id));
    assert!(!page.cancel_color_dialog(id));
    assert_eq!(page.rows()[0].kind, OptionKind::Color { value: 0x123456 });
    assert_eq!(calls.borrow().as_slice(), &[vec![Value::Int(id as i32)]]);
}

#[test]
fn ui_13_color_accept_notifies_then_script_value_update_applies() {
    let (mut game, calls) = game_with_probe();
    let mut page = ConfigPage::new(0);
    let id = page.add_color("Tint", 0x123456, 0).unwrap();

    assert!(page.open_color_dialog(&mut game, SCRIPT, id));
    assert!(page.accept_color(&mut game, SCRIPT, id, 0xabcdef));
    assert_eq!(page.rows()[0].kind, OptionKind::Color { value: 0x123456 });
    assert!(page.set_color_value(id, 0xabcdef));
    assert_eq!(page.rows()[0].kind, OptionKind::Color { value: 0xabcdef });
    assert_eq!(
        calls.borrow().as_slice(),
        &[
            vec![Value::Int(id as i32)],
            vec![Value::Int(id as i32), Value::Int(0xabcdef)],
        ]
    );
}
