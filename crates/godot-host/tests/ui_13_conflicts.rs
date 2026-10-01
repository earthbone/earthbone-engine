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
        .add("Ui13.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&calls);
    assert!(game.vm.natives.register_global(
        "Ui13ConflictProbe",
        "Record",
        Box::new(move |call| {
            captured.borrow_mut().push(call.args.clone());
            NativeReturn::Value(Value::None)
        }),
    ));
    let callback = FunctionBuilder::new("OnOptionKeyMapChange", "None")
        .param("option", "Int")
        .param("keyCode", "Int")
        .param("conflictControl", "String")
        .param("conflictName", "String")
        .ins(
            Opcode::CallStatic,
            vec![
                s("Ui13ConflictProbe"),
                s("Record"),
                ident("::nonevar"),
                ident("option"),
                ident("keyCode"),
                ident("conflictControl"),
                ident("conflictName"),
            ],
        )
        .build();
    game.load_script(
        &ScriptBuilder::new("Ui13ConflictConfig", "")
            .function(callback)
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(SCRIPT);
    game.vm.attach(object, "Ui13ConflictConfig").unwrap();
    (game, calls)
}

#[test]
fn ui_13_vanilla_conflict_precedes_mod_conflict_and_preserves_key_value() {
    let (mut game, calls) = game_with_probe();
    let mut page = ConfigPage::new(0);
    let id = page.add_keymap("Dodge", 42, 0).unwrap();
    let mut scanner = KeyConflictScanner::default();
    scanner.set_vanilla_control(44, "Jump");
    let config = scanner.register_mod_config("Other Mod");
    assert!(scanner.set_custom_control(config, 44, "Dodge"));

    assert!(page.remap_key_scanned(&mut game, SCRIPT, id, 44, &scanner));
    assert_eq!(
        calls.borrow().as_slice(),
        &[vec![
            Value::Int(id as i32),
            Value::Int(44),
            Value::string("Jump"),
            Value::string(""),
        ]]
    );
    assert_eq!(page.rows()[0].kind, OptionKind::KeyMap { key_code: 42 });
}

#[test]
fn ui_13_first_registered_mod_conflict_supplies_config_name() {
    let (mut game, calls) = game_with_probe();
    let mut page = ConfigPage::new(0);
    let id = page.add_keymap("Dodge", 42, 0).unwrap();
    let mut scanner = KeyConflictScanner::default();
    let first = scanner.register_mod_config("First Mod");
    let second = scanner.register_mod_config("Second Mod");
    assert!(scanner.set_custom_control(first, 44, "Roll"));
    assert!(scanner.set_custom_control(second, 44, "Sprint"));

    assert!(page.remap_key_scanned(&mut game, SCRIPT, id, 44, &scanner));
    assert_eq!(
        calls.borrow().as_slice(),
        &[vec![
            Value::Int(id as i32),
            Value::Int(44),
            Value::string("Roll"),
            Value::string("First Mod"),
        ]]
    );
}

#[test]
fn ui_13_unmapped_key_sends_empty_conflict_fields() {
    let (mut game, calls) = game_with_probe();
    let mut page = ConfigPage::new(0);
    let id = page.add_keymap("Dodge", 42, 0).unwrap();
    let scanner = KeyConflictScanner::default();

    assert!(page.remap_key_scanned(&mut game, SCRIPT, id, 44, &scanner));
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
