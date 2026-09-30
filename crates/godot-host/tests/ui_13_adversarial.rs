// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OptionKind};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{NativeReturn, Opcode, Value};

const SCRIPT: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x123,
};

fn game() -> Game {
    let mut order = LoadOrder::new();
    order.add("Ui13.esp", Plugin::new(FileHeader::default())).unwrap();
    Game::new(order)
}

fn callback_probe(game: &mut Game) -> Rc<RefCell<Vec<Vec<Value>>>> {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let target = Rc::clone(&calls);
    assert!(game.vm.natives.register_global(
        "Ui13Adversarial",
        "Record",
        Box::new(move |call| {
            target.borrow_mut().push(call.args.clone());
            NativeReturn::Value(Value::None)
        }),
    ));
    let record_call = |params: &[(&str, &str)]| {
        let mut function = FunctionBuilder::new("OnOptionKeyMapChange", "None");
        for (name, kind) in params {
            function = function.param(*name, kind);
        }
        function
            .ins(
                Opcode::CallStatic,
                vec![
                    s("Ui13Adversarial"),
                    s("Record"),
                    ident("::nonevar"),
                    ident("option"),
                    ident("keyCode"),
                    ident("conflictControl"),
                    ident("conflictName"),
                ],
            )
            .build()
    };
    let keymap = record_call(&[
        ("option", "Int"),
        ("keyCode", "Int"),
        ("conflictControl", "String"),
        ("conflictName", "String"),
    ]);
    let color_open = FunctionBuilder::new("OnOptionColorOpen", "None")
        .param("option", "Int")
        .ins(
            Opcode::CallStatic,
            vec![
                s("Ui13Adversarial"),
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
                s("Ui13Adversarial"),
                s("Record"),
                ident("::nonevar"),
                ident("option"),
                ident("color"),
            ],
        )
        .build();
    game.load_script(
        &ScriptBuilder::new("Ui13AdversarialConfig", "")
            .function(keymap)
            .function(color_open)
            .function(color_accept)
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(SCRIPT);
    game.vm.attach(object, "Ui13AdversarialConfig").unwrap();
    calls
}

#[test]
fn ui_13_keymap_forwards_both_conflict_fields_and_keeps_value_until_script_writes() {
    let mut game = game();
    let calls = callback_probe(&mut game);
    let mut page = ConfigPage::new(0);
    let id = page.add_keymap("Dodge", 42, 0).unwrap();

    assert!(page.remap_key(&mut game, SCRIPT, id, 44, "Jump", "Other Mod"));
    assert_eq!(calls.borrow().as_slice(), &[vec![
        Value::Int(id as i32),
        Value::Int(44),
        Value::string("Jump"),
        Value::string("Other Mod"),
    ]]);
    assert_eq!(page.rows()[0].kind, OptionKind::KeyMap { key_code: 42 });
}

#[test]
fn ui_13_color_cancel_rejects_stale_accept_and_can_reopen() {
    let mut game = game();
    let calls = callback_probe(&mut game);
    let mut page = ConfigPage::new(0);
    let id = page.add_color("Tint", 0x1122_3344, 0).unwrap();

    assert!(page.open_color_dialog(&mut game, SCRIPT, id));
    assert_eq!(calls.borrow().as_slice(), &[vec![Value::Int(id as i32)]]);
    assert!(page.cancel_color_dialog(id));
    assert!(!page.accept_color(&mut game, SCRIPT, id, 0xaabb_ccdd_u32 as i32));
    assert_eq!(page.rows()[0].kind, OptionKind::Color { value: 0x1122_3344 });
    assert_eq!(calls.borrow().len(), 1);
    assert!(page.open_color_dialog(&mut game, SCRIPT, id));
    assert!(page.accept_color(&mut game, SCRIPT, id, 0xaabb_ccdd_u32 as i32));
    assert_eq!(calls.borrow().as_slice(), &[
        vec![Value::Int(id as i32)],
        vec![Value::Int(id as i32)],
        vec![Value::Int(id as i32), Value::Int(0xaabb_ccdd_u32 as i32)],
    ]);
}

#[test]
fn ui_13_disabled_and_hidden_options_do_not_emit_callbacks() {
    let mut game = game();
    let calls = callback_probe(&mut game);
    let mut page = ConfigPage::new(0);
    let disabled = page.add_keymap("Disabled", 1, 0x01).unwrap();
    let hidden = page.add_color("Hidden", 0x0102_0304, 0x02).unwrap();

    assert!(!page.remap_key(&mut game, SCRIPT, disabled, 2, "", ""));
    assert!(!page.open_color_dialog(&mut game, SCRIPT, hidden));
    assert!(calls.borrow().is_empty());
}
