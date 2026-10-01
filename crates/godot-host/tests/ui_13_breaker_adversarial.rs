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
        .add("Ui13Breaker.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&calls);
    assert!(game.vm.natives.register_global(
        "Ui13BreakerProbe",
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
                s("Ui13BreakerProbe"),
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
                s("Ui13BreakerProbe"),
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
                s("Ui13BreakerProbe"),
                s("Record"),
                ident("::nonevar"),
                ident("option"),
                ident("color"),
            ],
        )
        .build();
    game.load_script(
        &ScriptBuilder::new("Ui13BreakerConfig", "")
            .function(key_callback)
            .function(color_open)
            .function(color_accept)
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(SCRIPT);
    game.vm.attach(object, "Ui13BreakerConfig").unwrap();
    (game, calls)
}

#[test]
fn ui_13_repeated_keymap_events_rescan_updated_conflicts() {
    let (mut game, calls) = game_with_probe();
    let mut page = ConfigPage::new(0);
    let id = page.add_keymap("Dodge", 42, 0).unwrap();
    let mut scanner = KeyConflictScanner::default();
    scanner.set_vanilla_control(44, "Jump");

    assert!(page.remap_key_scanned(&mut game, SCRIPT, id, 44, &scanner));
    scanner.set_vanilla_control(44, "");
    let config = scanner.register_mod_config("Combat Mod");
    assert!(scanner.set_custom_control(config, 44, "Block"));
    assert!(page.remap_key_scanned(&mut game, SCRIPT, id, 44, &scanner));

    assert_eq!(
        calls.borrow().as_slice(),
        &[
            vec![
                Value::Int(id as i32),
                Value::Int(44),
                Value::string("Jump"),
                Value::string(""),
            ],
            vec![
                Value::Int(id as i32),
                Value::Int(44),
                Value::string("Block"),
                Value::string("Combat Mod"),
            ],
        ]
    );
    assert_eq!(page.rows()[0].kind, OptionKind::KeyMap { key_code: 42 });
}

#[test]
fn ui_13_color_callback_uses_page_option_id_and_cancel_keeps_signed_color() {
    let (mut game, calls) = game_with_probe();
    let mut page = ConfigPage::new(3);
    let id = page.add_color("Tint", i32::MIN + 7, 0).unwrap();

    assert_eq!(id, 3 * 0x100);
    assert!(page.open_color_dialog(&mut game, SCRIPT, id));
    assert!(page.cancel_color_dialog(id));
    assert_eq!(page.rows()[0].kind, OptionKind::Color { value: i32::MIN + 7 });
    assert_eq!(calls.borrow().as_slice(), &[vec![Value::Int(id as i32)]]);
    assert!(!page.accept_color(&mut game, SCRIPT, id, 99));
    assert_eq!(calls.borrow().len(), 1);
}

#[test]
fn ui_13_color_accept_requires_a_fresh_open_after_cancel() {
    let (mut game, calls) = game_with_probe();
    let mut page = ConfigPage::new(0);
    let id = page.add_color("Tint", 0x123456, 0).unwrap();

    assert!(page.open_color_dialog(&mut game, SCRIPT, id));
    assert!(page.cancel_color_dialog(id));
    assert!(!page.accept_color(&mut game, SCRIPT, id, 0xabcdef));
    assert!(page.open_color_dialog(&mut game, SCRIPT, id));
    assert!(page.accept_color(&mut game, SCRIPT, id, 0xabcdef));
    assert_eq!(
        calls.borrow().as_slice(),
        &[
            vec![Value::Int(id as i32)],
            vec![Value::Int(id as i32)],
            vec![Value::Int(id as i32), Value::Int(0xabcdef)],
        ]
    );
    assert_eq!(page.rows()[0].kind, OptionKind::Color { value: 0x123456 });
}
