#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OptionKind, OPTION_FLAG_DISABLED};
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    NativeReturn, Opcode, Value,
};

fn game() -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Fixture.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    Game::new(order)
}

fn script() -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: 0x123,
    }
}

fn attach_callback_probe(game: &mut Game) -> Rc<RefCell<Vec<Vec<Value>>>> {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&calls);
    assert!(game.vm.natives.register_global(
        "Ui13CallbackProbe",
        "Record",
        Box::new(move |call| {
            captured.borrow_mut().push(call.args.clone());
            NativeReturn::Value(Value::None)
        }),
    ));
    let keymap = FunctionBuilder::new("OnOptionKeyMapChange", "None")
        .param("option", "Int")
        .param("keyCode", "Int")
        .param("conflictControl", "String")
        .param("conflictName", "String")
        .ins(
            Opcode::CallStatic,
            vec![
                s("Ui13CallbackProbe"),
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
                s("Ui13CallbackProbe"),
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
                s("Ui13CallbackProbe"),
                s("Record"),
                ident("::nonevar"),
                ident("option"),
                ident("color"),
            ],
        )
        .build();
    game.load_script(
        &ScriptBuilder::new("Ui13Config", "")
            .function(keymap)
            .function(color_open)
            .function(color_accept)
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(script());
    game.vm.attach(object, "Ui13Config").unwrap();
    calls
}

#[test]
fn ui_13_keymap_conflicts_reach_the_config_callback_without_mutating_value() {
    let mut game = game();
    let calls = attach_callback_probe(&mut game);
    let mut page = ConfigPage::new(1);
    let id = page.add_keymap("Dodge", 42, 0).unwrap();

    assert!(page.remap_key(&mut game, script(), id, 44, "Jump", "Other Mod"));
    assert_eq!(
        calls.borrow().as_slice(),
        &[vec![
            Value::Int(id as i32),
            Value::Int(44),
            Value::string("Jump"),
            Value::string("Other Mod"),
        ]]
    );
    assert_eq!(page.rows()[0].kind, OptionKind::KeyMap { key_code: 42 });
    assert!(!page.remap_key(&mut game, script(), id + 1, 44, "", ""));
    assert!(page.set_keymap_value(id, 44));
    assert_eq!(page.rows()[0].kind, OptionKind::KeyMap { key_code: 44 });
}

#[test]
fn ui_13_color_accept_follows_open_then_accept_and_updates_only_on_script_write() {
    let mut game = game();
    let calls = attach_callback_probe(&mut game);
    let mut page = ConfigPage::new(0);
    let id = page.add_color("Tint", 0x1122_3344, 0).unwrap();

    assert!(page.open_color_dialog(&mut game, script(), id));
    assert_eq!(calls.borrow().as_slice(), &[vec![Value::Int(id as i32)]]);
    assert!(!page.open_color_dialog(&mut game, script(), id));
    assert!(page.accept_color(&mut game, script(), id, 0xaabb_ccdd_u32 as i32));
    assert_eq!(
        calls.borrow().as_slice(),
        &[
            vec![Value::Int(id as i32)],
            vec![Value::Int(id as i32), Value::Int(0xaabb_ccdd_u32 as i32)],
        ]
    );
    assert_eq!(
        page.rows()[0].kind,
        OptionKind::Color { value: 0x1122_3344 }
    );
    assert!(page.set_color_value(id, 0xaabb_ccdd_u32 as i32));
    assert_eq!(
        page.rows()[0].kind,
        OptionKind::Color {
            value: 0xaabb_ccdd_u32 as i32
        }
    );
}

#[test]
fn ui_13_color_cancel_leaves_value_unchanged_and_rejects_late_accept() {
    let mut game = game();
    let calls = attach_callback_probe(&mut game);
    let mut page = ConfigPage::new(2);
    let id = page.add_color("Tint", 0x0102_0304, 0).unwrap();

    assert!(page.open_color_dialog(&mut game, script(), id));
    assert_eq!(calls.borrow().len(), 1);
    assert!(page.cancel_color_dialog(id));
    assert!(!page.accept_color(&mut game, script(), id, 0x0506_0708));
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(
        page.rows()[0].kind,
        OptionKind::Color { value: 0x0102_0304 }
    );
    assert!(!page.cancel_color_dialog(id));
}

#[test]
fn ui_13_disabled_color_and_keymap_rows_reject_callbacks() {
    let mut game = game();
    let mut page = ConfigPage::new(0);
    let color = page
        .add_color("Tint", 0x0102_0304, OPTION_FLAG_DISABLED)
        .unwrap();
    let keymap = page.add_keymap("Dodge", 42, OPTION_FLAG_DISABLED).unwrap();

    assert!(!page.open_color_dialog(&mut game, script(), color));
    assert!(!page.remap_key(&mut game, script(), keymap, 44, "", ""));
}
