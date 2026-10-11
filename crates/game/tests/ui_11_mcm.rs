// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SkyUI text/toggle presentation protocol and original-framework execution.
//! Buffer and selection semantics follow local SKI_ConfigBase and ConfigPanel.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::value::{ArrayRef, ElementKind};
use soe_papyrus::Opcode::{Assign, CallStatic, IAdd};
use soe_papyrus::Value;

use crate::support::{ui_05_fixture as compiled, ui_11_fixture};

const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const ROOT: &str = "_root.ConfigPanelFader.configPanel.";

fn array(kind: ElementKind, values: &[Value]) -> Value {
    let array = ArrayRef::new(kind, 0);
    *array.items.borrow_mut() = values.to_vec();
    Value::Array(array)
}

pub(super) fn protocol_game() -> Game {
    let mut game = protocol_game_unopened();
    assert!(game.mcm_open(FORM));
    game
}

pub(super) fn protocol_game_unopened() -> Game {
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("SKI_ConfigBase")],
        ..Default::default()
    };
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        quest::build_record(FORM.object, "Ui11Protocol", flags::START_GAME_ENABLED)
            .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST"))),
    );
    let mut order = LoadOrder::new();
    order.add("Ui11Protocol.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let mut script = ScriptBuilder::new("SKI_ConfigBase", "Quest")
        .auto_property("ModName", "String", s("Protocol Mod"))
        .auto_property("Pages", "String[]", none())
        .variable("_currentPageNum", "Int", i(1))
        .variable("selection_count", "Int", i(0))
        .variable("selected_option", "Int", i(-1))
        .function(FunctionBuilder::new("OpenConfig", "None").build())
        .function(FunctionBuilder::new("CloseConfig", "None").build())
        .function(
            FunctionBuilder::new("OnOptionSelect", "None")
                .param("option", "Int")
                .ins(
                    IAdd,
                    vec![ident("selection_count"), ident("selection_count"), i(1)],
                )
                .ins(Assign, vec![ident("selected_option"), ident("option")])
                .build(),
        );
    for method in [
        "InvokeIntA",
        "InvokeStringA",
        "InvokeFloatA",
        "InvokeInt",
        "SetInt",
        "SetString",
        "SetFloat",
    ] {
        script = script.function(
            FunctionBuilder::new(method, "None")
                .param("target", "String")
                .param("value", "None")
                .ins(
                    CallStatic,
                    vec![
                        s("UI"),
                        s(method),
                        ident("::nonevar"),
                        s("Journal Menu"),
                        ident("target"),
                        ident("value"),
                    ],
                )
                .build(),
        );
    }
    game.load_script(&script.bytes()).unwrap();
    game.start();
    game
}

fn invoke(game: &mut Game, method: &str, target: &str, value: Value) {
    let object = game.object_of_form(FORM);
    let thread = game
        .vm
        .call(
            object,
            method,
            &[Value::string(&format!("{ROOT}{target}")), value],
        )
        .unwrap();
    game.run_until_idle(64);
    assert!(game.vm.thread_result(thread).is_some());
}

fn present(game: &mut Game) {
    invoke(
        game,
        "InvokeIntA",
        "setOptionFlagsBuffer",
        array(
            ElementKind::Int,
            &[
                Value::Int(2),
                Value::Int(0x103),
                Value::Int(0),
                Value::Int(0),
                Value::Int(0x202),
            ],
        ),
    );
    invoke(
        game,
        "InvokeStringA",
        "setOptionTextBuffer",
        array(
            ElementKind::Str,
            &[
                Value::string("Status"),
                Value::string("Enabled"),
                Value::string(""),
                Value::string(""),
                Value::string("Hidden"),
            ],
        ),
    );
    invoke(
        game,
        "InvokeStringA",
        "setOptionStrValueBuffer",
        array(
            ElementKind::Str,
            &[
                Value::string("Ready"),
                Value::string(""),
                Value::string(""),
                Value::string(""),
                Value::string("Secret"),
            ],
        ),
    );
    invoke(
        game,
        "InvokeFloatA",
        "setOptionNumValueBuffer",
        array(
            ElementKind::Float,
            &[
                Value::Float(0.0),
                Value::Float(1.0),
                Value::Float(0.0),
                Value::Float(0.0),
                Value::Float(0.0),
            ],
        ),
    );
    invoke(game, "InvokeInt", "flushOptionBuffers", Value::Int(5));
}

fn counter(game: &mut Game, script: &str, name: &str) -> Value {
    let object = game.object_of_form(FORM);
    game.vm.get_var(object, script, name).unwrap()
}

#[test]
fn ui_11_script_buffers_preserve_text_toggle_flags_and_sparse_slots() {
    let mut game = protocol_game();
    present(&mut game);
    let options = game.mcm_options();
    assert_eq!(
        options
            .iter()
            .map(|row| (row.index, row.option_type, row.flags))
            .collect::<Vec<_>>(),
        [(0, 2, 0), (1, 3, 1), (2, 0, 0), (3, 0, 0), (4, 2, 2)]
    );
    assert_eq!(
        (
            options[0].text.as_str(),
            options[0].string_value.as_str(),
            options[1].number_value
        ),
        ("Status", "Ready", 1.0)
    );
}

#[test]
fn ui_11_script_flag_updates_reach_each_slot_in_call_order() {
    let mut game = protocol_game();
    present(&mut game);
    for (index, flags) in [(0, 1), (1, 1), (1, 0), (4, 3)] {
        invoke(
            &mut game,
            "InvokeIntA",
            "setOptionFlags",
            array(ElementKind::Int, &[Value::Int(index), Value::Int(flags)]),
        );
    }
    assert_eq!(
        game.mcm_options()
            .iter()
            .map(|row| row.flags)
            .collect::<Vec<_>>(),
        [1, 0, 0, 0, 3]
    );
}

#[test]
fn ui_11_script_cursor_updates_keep_text_and_toggle_values_separate() {
    let mut game = protocol_game();
    present(&mut game);
    invoke(&mut game, "SetInt", "optionCursorIndex", Value::Int(0));
    invoke(
        &mut game,
        "SetString",
        "optionCursor.strValue",
        Value::string("Selected"),
    );
    invoke(&mut game, "SetInt", "optionCursorIndex", Value::Int(1));
    invoke(
        &mut game,
        "SetFloat",
        "optionCursor.numValue",
        Value::Float(0.0),
    );
    let options = game.mcm_options();
    assert_eq!(
        (options[0].string_value.as_str(), options[1].number_value),
        ("Selected", 0.0)
    );
}

#[test]
fn ui_11_selection_rejects_disabled_hidden_stale_and_unknown_ids() {
    let mut game = protocol_game();
    present(&mut game);
    for id in [0x101, 0x104, 0x102, 0x17f, 0, 0x200, u32::MAX] {
        assert!(!game.mcm_select_option(FORM, id), "accepted {id:#x}");
    }
    assert!(game.mcm_select_option(FORM, 0x100));
    assert_eq!(
        counter(&mut game, "SKI_ConfigBase", "selection_count"),
        Value::Int(1)
    );
    assert_eq!(
        counter(&mut game, "SKI_ConfigBase", "selected_option"),
        Value::Int(0x100)
    );
}

#[test]
fn ui_11_text_and_toggle_dispatch_once_per_press_without_changing_values() {
    let mut game = protocol_game();
    present(&mut game);
    invoke(
        &mut game,
        "InvokeIntA",
        "setOptionFlags",
        array(ElementKind::Int, &[Value::Int(1), Value::Int(4)]),
    );
    let before = game.mcm_options();
    for (count, id) in [(1, 0x100), (2, 0x101), (3, 0x101)] {
        assert!(game.mcm_select_option(FORM, id));
        assert_eq!(
            counter(&mut game, "SKI_ConfigBase", "selection_count"),
            Value::Int(count)
        );
        assert_eq!(
            counter(&mut game, "SKI_ConfigBase", "selected_option"),
            Value::Int(id as i32)
        );
        assert_eq!(game.mcm_options(), before);
    }
}

#[test]
fn ui_11_bad_updates_and_flushes_preserve_current_rows() {
    let mut game = protocol_game();
    present(&mut game);
    let before = game.mcm_options();
    invoke(
        &mut game,
        "InvokeIntA",
        "setOptionFlags",
        array(ElementKind::Int, &[Value::Int(-1), Value::Int(1)]),
    );
    invoke(&mut game, "SetInt", "optionCursorIndex", Value::Int(128));
    invoke(
        &mut game,
        "SetString",
        "optionCursor.strValue",
        Value::string("Wrong"),
    );
    invoke(
        &mut game,
        "InvokeInt",
        "flushOptionBuffers",
        Value::Int(129),
    );
    invoke(
        &mut game,
        "InvokeIntA",
        "setOptionFlagsBuffer",
        array(ElementKind::Int, &[Value::Int(2)]),
    );
    invoke(&mut game, "InvokeInt", "flushOptionBuffers", Value::Int(5));
    assert_eq!(game.mcm_options(), before);
    invoke(&mut game, "SetInt", "optionCursorIndex", Value::Int(1));
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        invoke(
            &mut game,
            "SetFloat",
            "optionCursor.numValue",
            Value::Float(value),
        );
        assert_eq!(game.mcm_options(), before);
    }
}

#[test]
fn ui_11_close_discards_presentation_before_reopening() {
    let mut game = protocol_game();
    present(&mut game);
    game.mcm_close();
    assert!(game.mcm_options().is_empty());
    assert!(game.mcm_open(FORM));
    assert!(game.mcm_options().is_empty());
}

#[test]
fn ui_11_unmodified_skyui_pex_adds_updates_and_selects_once() {
    let Some(fixture) = ui_11_fixture::load() else {
        return;
    };
    let mut game = fixture.game();
    assert_eq!(game.mcm_configs()[0].name, "UI-11 Mod");
    assert!(game.mcm_open(FORM));
    assert!(game.mcm_select_page(0));
    assert_eq!(game.mcm_page_number(), Some(1));
    assert_eq!(
        counter(&mut game, compiled::CHILD, "text_id"),
        Value::Int(0x100)
    );
    assert_eq!(
        counter(&mut game, compiled::CHILD, "toggle_id"),
        Value::Int(0x101)
    );
    let options = game.mcm_options();
    assert_eq!(
        (options[0].text.as_str(), options[0].string_value.as_str()),
        ("Status", "Ready")
    );
    assert_eq!(
        (
            options[1].option_type,
            options[1].number_value,
            options[1].flags
        ),
        (3, 1.0, 1)
    );
    assert_eq!((options[4].index, options[4].flags), (4, 2));
    assert!(!game.mcm_select_option(FORM, 0x101));
    assert!(!game.mcm_select_option(FORM, 0x104));
    assert!(!game.mcm_select_option(FORM, 0));

    let object = game.object_of_form(FORM);
    for method in ["EnableOptions", "DisableOptions", "EnableOptions"] {
        let thread = game.vm.call(object, method, &[]).unwrap();
        game.run_until_idle(64);
        assert!(game.vm.thread_result(thread).is_some());
        let expected = u32::from(method == "DisableOptions");
        assert_eq!(
            (game.mcm_options()[0].flags, game.mcm_options()[1].flags),
            (expected, expected)
        );
    }
    assert!(game.mcm_select_option(FORM, 0x100));
    assert_eq!(
        counter(&mut game, compiled::CHILD, "selection_count"),
        Value::Int(1)
    );
    assert_eq!(
        counter(&mut game, compiled::CHILD, "selected_option"),
        Value::Int(0x100)
    );
    let options = game.mcm_options();
    assert_eq!((options[0].flags, options[1].flags), (1, 0));
    assert_eq!(
        (options[0].string_value.as_str(), options[1].number_value),
        ("Selected", 0.0)
    );
    assert!(!game.mcm_select_option(FORM, 0x100));
    assert_eq!(
        counter(&mut game, compiled::CHILD, "selection_count"),
        Value::Int(1)
    );
    assert!(game.mcm_select_option(FORM, 0x101));
    assert_eq!(
        counter(&mut game, compiled::CHILD, "selection_count"),
        Value::Int(2)
    );
    assert_eq!(
        counter(&mut game, compiled::CHILD, "selected_option"),
        Value::Int(0x101)
    );
    assert_eq!(game.mcm_options()[1].number_value, 0.0);
    assert!(game.mcm_select_page(1));
    assert_eq!(game.mcm_page_number(), Some(2));
    assert!(!game.mcm_select_option(FORM, 0x100));
    assert!(game.mcm_select_option(FORM, 0x200));
    assert_eq!(
        counter(&mut game, compiled::CHILD, "selection_count"),
        Value::Int(3)
    );
    assert_eq!(
        counter(&mut game, compiled::CHILD, "selected_option"),
        Value::Int(0x200)
    );
    assert!(
        !game
            .vm
            .log
            .messages()
            .iter()
            .any(|message| message.starts_with("error:")),
        "{:?}",
        game.vm.log.messages()
    );
}

#[test]
fn ui_11_unmodified_skyui_state_options_update_and_restore_state_once() {
    let Some(fixture) = ui_11_fixture::load_state_options() else {
        return;
    };
    let mut game = fixture.game();
    assert!(game.mcm_open(FORM));
    assert!(game.mcm_select_page(0));
    let options = game.mcm_options();
    assert_eq!(
        options
            .iter()
            .map(|row| (row.index, row.option_type))
            .collect::<Vec<_>>(),
        [(0, 2), (1, 0), (2, 3)]
    );
    let object = game.object_of_form(FORM);
    let old_state = game.vm.state_of(object, compiled::CHILD);
    for (id, counter_name) in [(0x100, "text_state_count"), (0x102, "toggle_state_count")] {
        assert!(game.mcm_select_option(FORM, id));
        assert_eq!(
            counter(&mut game, compiled::CHILD, counter_name),
            Value::Int(1)
        );
        assert_eq!(game.vm.state_of(object, compiled::CHILD), old_state);
        assert!(!game.mcm_select_option(FORM, id));
        assert_eq!(
            counter(&mut game, compiled::CHILD, counter_name),
            Value::Int(1)
        );
    }
    assert_eq!(
        counter(&mut game, compiled::CHILD, "selection_count"),
        Value::Int(0)
    );
    let options = game.mcm_options();
    assert_eq!(
        (options[0].string_value.as_str(), options[0].flags),
        ("Selected", 1)
    );
    assert_eq!((options[2].number_value, options[2].flags), (0.0, 1));
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
