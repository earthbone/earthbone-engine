// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-07 conformance: `Debug.TraceUser`, `Debug.TraceConditional` and
//! `Debug.TraceStack` share `text_world`'s form id conversion with
//! `Debug.Trace` (`crates/game/src/natives.rs`, the `debug` registration
//! function). `Debug_Script.wikitext` (lines 79-90) defines their object
//! argument position:
//! - `TraceUser(String asUserLog, String asTextToPrint, Int aiSeverity)`,
//!   the traced value is argument 1.
//! - `TraceConditional(String TextToPrint, Bool ShowTrace)`, the traced
//!   value is argument 0, only logged when `ShowTrace` is true.
//! - `TraceStack(String asTextToPrint, Int aiSeverity)`, the traced value
//!   is argument 0.
//!
//! Each one, like `Debug.Trace`, converts a non-string argument
//! automatically, so an object argument must carry the real 8-hex-digit
//! runtime FormID the way the game's own log does, not the engine's
//! internal load order notation.

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Handle};
use soe_papyrus::asm::{b, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");
const MARKER: u32 = 0x800;

/// One master plugin with a single `STAT` form named `Marker`, loaded and
/// started. The plugin is the only one in the load order, so its file
/// index is 0 and `Marker`'s runtime FormID is the raw object id, printed
/// as `00000800`.
fn game_with_marker() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"STAT"), MARKER).zsub(EDID, "Marker"));
    let mut order = LoadOrder::new();
    order.add("Test.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.start();
    game
}

/// A one-off global script whose `Run` function calls `Debug.TraceUser`
/// with a fixed user log name and severity, the way a vanilla script's
/// `CallStatic` would.
fn trace_user_script() -> Vec<u8> {
    let run = FunctionBuilder::new("Run", "None")
        .global()
        .param("t", "Form")
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TraceUser"),
                ident("::nonevar"),
                s("UserLog"),
                ident("t"),
                i(0),
            ],
        )
        .build();
    ScriptBuilder::new("TraceUserProbe", "Form")
        .function(run)
        .bytes()
}

/// A one-off global script whose `Run` function calls
/// `Debug.TraceConditional` with `ShowTrace` true, the traced value in
/// argument position 0.
fn trace_conditional_script(show: bool) -> Vec<u8> {
    let run = FunctionBuilder::new("Run", "None")
        .global()
        .param("t", "Form")
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TraceConditional"),
                ident("::nonevar"),
                ident("t"),
                b(show),
            ],
        )
        .build();
    ScriptBuilder::new("TraceConditionalProbe", "Form")
        .function(run)
        .bytes()
}

/// A one-off global script whose `Run` function calls `Debug.TraceStack`
/// with a fixed severity, the traced value in argument position 0.
fn trace_stack_script() -> Vec<u8> {
    let run = FunctionBuilder::new("Run", "None")
        .global()
        .param("t", "Form")
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TraceStack"),
                ident("::nonevar"),
                ident("t"),
                i(0),
            ],
        )
        .build();
    ScriptBuilder::new("TraceStackProbe", "Form")
        .function(run)
        .bytes()
}

/// Loads `script`, runs `<Class>.Run(target)` to completion and hands back
/// the papyrus log's messages, in order.
fn run_trace(game: &mut Game, script: &[u8], class: &str, target: Value) -> Vec<String> {
    game.load_script(script).unwrap();
    let thread = game.vm.call_global(class, "Run", &[target]).unwrap();
    game.run_until_idle(64);
    let _ = thread;
    game.vm
        .log
        .messages()
        .iter()
        .map(|m| m.to_string())
        .collect()
}

/// Done check, `Debug.TraceUser`: an object argument (position 1) carries
/// its form id in the traced message.
#[test]
fn scr_07_trace_user_object_argument_carries_form_id() {
    let mut game = game_with_marker();
    let marker = game.form("Marker").expect("Marker form present");
    let object = game.object_of_form(marker);
    let messages = run_trace(
        &mut game,
        &trace_user_script(),
        "TraceUserProbe",
        Value::Object(object),
    );
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(
        messages[0].contains("00000800"),
        "TraceUser message should carry the runtime form id 00000800, got {:?}",
        messages[0]
    );
}

/// `Debug.TraceUser` on a quest alias, which has no form id, falls back to
/// the bare object display instead of fabricating one.
#[test]
fn scr_07_trace_user_alias_receiver_falls_back_without_form_id() {
    let mut game = game_with_marker();
    let marker = game.form("Marker").expect("Marker form present");
    let alias = game.object_of(Handle::Alias {
        quest: marker,
        id: 0,
    });
    let messages = run_trace(
        &mut game,
        &trace_user_script(),
        "TraceUserProbe",
        Value::Object(alias),
    );
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(
        !messages[0].contains(':'),
        "an alias has no form id, TraceUser message should not carry one, got {:?}",
        messages[0]
    );
}

/// `Debug.TraceUser` with a `None` object argument traces as an empty
/// message, unaffected by the form id change.
#[test]
fn scr_07_trace_user_none_argument_is_unaffected() {
    let mut game = game_with_marker();
    let messages = run_trace(
        &mut game,
        &trace_user_script(),
        "TraceUserProbe",
        Value::None,
    );
    assert_eq!(messages, vec![String::new()], "{messages:?}");
}

/// Done check, `Debug.TraceConditional`: with `ShowTrace` true, an object
/// argument carries its form id in the traced message.
#[test]
fn scr_07_trace_conditional_object_argument_carries_form_id() {
    let mut game = game_with_marker();
    let marker = game.form("Marker").expect("Marker form present");
    let object = game.object_of_form(marker);
    let messages = run_trace(
        &mut game,
        &trace_conditional_script(true),
        "TraceConditionalProbe",
        Value::Object(object),
    );
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(
        messages[0].contains("00000800"),
        "TraceConditional message should carry the runtime form id 00000800, got {:?}",
        messages[0]
    );
}

/// `Debug.TraceConditional` with `ShowTrace` false logs nothing at all,
/// wiki-defined behaviour unrelated to and unbroken by the form id change.
#[test]
fn scr_07_trace_conditional_false_condition_logs_nothing() {
    let mut game = game_with_marker();
    let marker = game.form("Marker").expect("Marker form present");
    let object = game.object_of_form(marker);
    let messages = run_trace(
        &mut game,
        &trace_conditional_script(false),
        "TraceConditionalProbe",
        Value::Object(object),
    );
    assert!(messages.is_empty(), "{messages:?}");
}

/// `Debug.TraceConditional` on a quest alias, which has no form id, falls
/// back to the bare object display instead of fabricating one.
#[test]
fn scr_07_trace_conditional_alias_receiver_falls_back_without_form_id() {
    let mut game = game_with_marker();
    let marker = game.form("Marker").expect("Marker form present");
    let alias = game.object_of(Handle::Alias {
        quest: marker,
        id: 0,
    });
    let messages = run_trace(
        &mut game,
        &trace_conditional_script(true),
        "TraceConditionalProbe",
        Value::Object(alias),
    );
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(
        !messages[0].contains(':'),
        "an alias has no form id, TraceConditional message should not carry one, got {:?}",
        messages[0]
    );
}

/// Done check, `Debug.TraceStack`: an object argument carries its form id
/// in the traced message.
#[test]
fn scr_07_trace_stack_object_argument_carries_form_id() {
    let mut game = game_with_marker();
    let marker = game.form("Marker").expect("Marker form present");
    let object = game.object_of_form(marker);
    let messages = run_trace(
        &mut game,
        &trace_stack_script(),
        "TraceStackProbe",
        Value::Object(object),
    );
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(
        messages[0].contains("00000800"),
        "TraceStack message should carry the runtime form id 00000800, got {:?}",
        messages[0]
    );
}

/// `Debug.TraceStack` on a quest alias, which has no form id, falls back
/// to the bare object display instead of fabricating one.
#[test]
fn scr_07_trace_stack_alias_receiver_falls_back_without_form_id() {
    let mut game = game_with_marker();
    let marker = game.form("Marker").expect("Marker form present");
    let alias = game.object_of(Handle::Alias {
        quest: marker,
        id: 0,
    });
    let messages = run_trace(
        &mut game,
        &trace_stack_script(),
        "TraceStackProbe",
        Value::Object(alias),
    );
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(
        !messages[0].contains(':'),
        "an alias has no form id, TraceStack message should not carry one, got {:?}",
        messages[0]
    );
}

/// `Debug.TraceStack` with a `None` object argument traces as an empty
/// message, unaffected by the form id change.
#[test]
fn scr_07_trace_stack_none_argument_is_unaffected() {
    let mut game = game_with_marker();
    let messages = run_trace(
        &mut game,
        &trace_stack_script(),
        "TraceStackProbe",
        Value::None,
    );
    assert_eq!(messages, vec![String::new()], "{messages:?}");
}
