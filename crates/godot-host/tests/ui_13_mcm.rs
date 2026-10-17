#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OptionKind, OPTION_FLAG_DISABLED};
use soe_papyrus::{
    asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder},
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
fn ui_13_papyrus_color_dialog_setters_update_host_parameters() {
    let mut game = game();
    let setter = |name: &str, slot| {
        FunctionBuilder::new(name, "None")
            .param("color", "Int")
            .ins(
                Opcode::ArraySetElement,
                vec![ident("_colorParams"), i(slot), ident("color")],
            )
            .build()
    };
    let on_open = FunctionBuilder::new("OnOptionColorOpen", "None")
        .param("option", "Int")
        .ins(Opcode::ArrayCreate, vec![ident("_colorParams"), i(2)])
        .ins(
            Opcode::CallMethod,
            vec![
                s("SetColorDialogStartColor"),
                ident("self"),
                ident("::nonevar"),
                i(0x123456),
            ],
        )
        .ins(
            Opcode::CallMethod,
            vec![
                s("SetColorDialogDefaultColor"),
                ident("self"),
                ident("::nonevar"),
                i(0xabcdef),
            ],
        )
        .build();
    game.load_script(
        &ScriptBuilder::new("SKI_ConfigBase", "")
            .variable("_colorParams", "Int[]", none())
            .function(setter("SetColorDialogStartColor", 0))
            .function(setter("SetColorDialogDefaultColor", 1))
            .function(on_open)
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(script());
    game.vm.attach(object, "SKI_ConfigBase").unwrap();

    let mut page = ConfigPage::new(0);
    let id = page.add_color("Tint", 0x010203, 0).unwrap();
    assert!(page.open_color_dialog(&mut game, script(), id));
    assert_eq!(
        page.color_dialog(),
        Some(soe_godot_host::mcm_menu::ColorDialog {
            option_id: id,
            start_color: 0x123456,
            default_color: 0xabcdef,
        })
    );
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

#[path = "../../game/tests/support/ui_05_fixture.rs"]
mod compiled;
#[path = "support/ui_13_fixture.rs"]
mod executable;

#[test]
fn ui_13_original_framework_color_setters_state_callbacks_and_key_conflicts() {
    for states in [false, true] {
        let Some(fixture) = executable::load(states) else {
            return;
        };
        let mut game = fixture.game();
        let form = GlobalFormId {
            file: 0,
            object: compiled::CONFIG,
        };
        assert!(game.mcm_open(form));
        assert!(game.mcm_select_page(0));
        let mut page = ConfigPage::from_game(&mut game).unwrap();
        assert_eq!(page.rows().len(), 2);
        let color_id = page.rows()[0].id;
        let key_id = page.rows()[1].id;
        assert_eq!(color_id, 256);
        assert_eq!(key_id, 257);
        assert_eq!(page.rows()[0].kind, OptionKind::Color { value: 0x112233 });
        assert_eq!(page.rows()[1].kind, OptionKind::KeyMap { key_code: 42 });
        assert!(page.open_color_dialog(&mut game, form, color_id));
        let dialog = page.color_dialog().unwrap();
        assert_eq!(dialog.start_color, 0x123456);
        assert_eq!(dialog.default_color, 0xabcdef);
        assert!(page.cancel_color_dialog(color_id));
        assert!(!page.accept_color(&mut game, form, color_id, 0));
        assert_eq!(
            ConfigPage::from_game(&mut game).unwrap().rows(),
            page.rows()
        );
        assert!(page.open_color_dialog(&mut game, form, color_id));
        assert!(page.accept_color(&mut game, form, color_id, 0x654321));
        assert!(!page.accept_color(&mut game, form, color_id, 0));
        assert_eq!(
            ConfigPage::from_game(&mut game).unwrap().rows()[0].kind,
            OptionKind::Color { value: 0x654321 }
        );
        let bindings = soe_godot_host::input::Bindings::vanilla_keyboard();
        assert!(page.remap_key_live(&mut game, form, key_id, 57, &bindings));
        let object = game.object_of_form(form);
        assert_eq!(
            game.vm.get_var(object, compiled::CHILD, "conflict_control"),
            Some(Value::string("Jump"))
        );
        assert_eq!(
            game.vm.get_var(object, compiled::CHILD, "conflict_name"),
            Some(Value::string(""))
        );
        assert!(page.remap_key_live(&mut game, form, key_id, 88, &bindings));
        assert_eq!(
            game.vm.get_var(object, compiled::CHILD, "conflict_control"),
            Some(Value::string("ModAction"))
        );
        assert_eq!(
            game.vm.get_var(object, compiled::CHILD, "conflict_name"),
            Some(Value::string("UI-13 Mod"))
        );
        assert_eq!(
            game.vm.get_var(object, compiled::CHILD, "color_open_count"),
            Some(Value::Int(2))
        );
        assert_eq!(
            game.vm
                .get_var(object, compiled::CHILD, "color_accept_count"),
            Some(Value::Int(1))
        );
        assert_eq!(
            game.vm.get_var(object, compiled::CHILD, "key_count"),
            Some(Value::Int(2))
        );
        assert_eq!(
            game.vm.state_of(object, compiled::CHILD),
            Some(String::new())
        );
        assert!(page.open_keymap_dialog(&mut game, form, key_id));
        assert!(page.cancel_active_dialog(key_id));
        game.mcm_close();
        assert!(!page.remap_key_live(&mut game, form, key_id, 57, &bindings));
        assert!(!page.open_color_dialog(&mut game, form, color_id));
        assert!(
            !game
                .vm
                .log
                .messages()
                .iter()
                .any(|message| message.starts_with("error:") || message.contains(" ERROR:")),
            "{:?}",
            game.vm.log.messages()
        );
    }
}

#[test]
fn ui_13_windowed_original_framework_dialogs() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let Some(fixture) = executable::load(false) else {
        return;
    };
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    fixture.write(directory.path());
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new(godot)
        .args([
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "res://ui_13_mcm_dialog_test.gd",
        ])
        .current_dir(&root)
        .env("SOE_UI_13_FIXTURE_DIR", directory.path())
        .output()
        .unwrap();
    let artifacts = root.join("target/ui-13");
    std::fs::create_dir_all(&artifacts).unwrap();
    std::fs::write(artifacts.join("stdout.txt"), &output.stdout).unwrap();
    std::fs::write(artifacts.join("stderr.txt"), &output.stderr).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{}\n{stdout}\n{stderr}",
        output.status
    );
    assert!(
        stdout.contains("PASS ui_13 native dialogs"),
        "{stdout}\n{stderr}"
    );
    std::fs::copy(
        directory.path().join("mcm-dialog.png"),
        artifacts.join("mcm-dialog.png"),
    )
    .unwrap();
}

#[test]
fn ui_13_live_conflicts_use_skse_mouse_and_gamepad_macro_codes() {
    use soe_godot_host::input::{Bindings, ControlMap};
    let mut game = game();
    let calls = attach_callback_probe(&mut game);
    let mut bindings = Bindings::vanilla_keyboard();
    bindings.apply_control_map(&ControlMap::parse(
        "WheelAction\t0xff\t0x08\t0xff\t1\t1\t1\nPadAction\t0xff\t0xff\t0x1000\t1\t1\t1\n",
    ));
    let mut page = ConfigPage::new(0);
    let id = page.add_keymap("Binding", 42, 0).unwrap();
    assert!(page.remap_key_live(&mut game, script(), id, 264, &bindings));
    assert!(page.remap_key_live(&mut game, script(), id, 276, &bindings));
    assert_eq!(calls.borrow()[0][2], Value::string("WheelAction"));
    assert_eq!(calls.borrow()[1][2], Value::string("PadAction"));
    assert_eq!(calls.borrow()[0][1], Value::Int(264));
    assert_eq!(calls.borrow()[1][1], Value::Int(276));
}
