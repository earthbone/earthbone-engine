// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-07: `Debug.Trace` and its relatives carry the form id of an object
//! argument in the trace output, the way the game's own log does.
//! `Debug_Script.wikitext` (lines 80-81) defines `Debug.Trace` as writing
//! its message to the papyrus log, converting a non-string argument to a
//! string automatically. This checks that an object argument's converted
//! text includes the form id `text_world` resolves, formatted as the real
//! 8-hex-digit runtime FormID (`World::runtime_id`), the way the game and
//! every external tool print it, not the engine's internal
//! `{file}:{object:06X}` load order notation from `GlobalFormId`'s
//! `Display` impl (`crates/esm/src/load_order.rs` lines 36 to 40).

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Handle};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;

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

/// A one-off global script whose `Run` function calls `Debug.Trace` on the
/// object it is handed, the way a vanilla script's `CallStatic` would.
fn trace_script() -> Vec<u8> {
    let run = FunctionBuilder::new("Run", "None")
        .global()
        .param("t", "Form")
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("Trace"),
                ident("::nonevar"),
                ident("t"),
                soe_papyrus::asm::i(0),
            ],
        )
        .build();
    ScriptBuilder::new("TraceProbe", "Form")
        .function(run)
        .bytes()
}

/// Runs `TraceProbe.Run(target)` to completion and hands back the papyrus
/// log's messages, in order.
fn trace(game: &mut Game, target: soe_papyrus::Value) -> Vec<String> {
    game.load_script(&trace_script()).unwrap();
    let thread = game.vm.call_global("TraceProbe", "Run", &[target]).unwrap();
    game.run_until_idle(64);
    let _ = thread;
    game.vm
        .log
        .messages()
        .iter()
        .map(|m| m.to_string())
        .collect()
}

/// Done check: an object printed to the log carries its form id, the way
/// the game prints it, when `Debug.Trace` receives an object argument.
#[test]
fn scr_07_trace_object_argument_carries_form_id() {
    let mut game = game_with_marker();
    let marker = game.form("Marker").expect("Marker form present");
    let object = game.object_of_form(marker);
    let messages = trace(&mut game, soe_papyrus::Value::Object(object));
    assert_eq!(messages.len(), 1, "{messages:?}");
    let msg = &messages[0];
    assert!(
        msg.contains("00000800"),
        "trace message should carry the runtime form id 00000800, got {msg:?}"
    );
    assert!(
        msg.contains(&object.0.to_string()),
        "trace message should still carry the object handle, got {msg:?}"
    );
}

/// `form_of` returns `None` for a quest alias, which has no form id
/// (`crates/game/src/world/objects.rs` lines 61-66). The invalid-receiver
/// case the wiki implies: `text_world` falls back to the bare object
/// display instead of panicking or fabricating a form id.
#[test]
fn scr_07_trace_alias_receiver_falls_back_without_form_id() {
    let mut game = game_with_marker();
    let marker = game.form("Marker").expect("Marker form present");
    let alias = game.object_of(Handle::Alias {
        quest: marker,
        id: 0,
    });
    let messages = trace(&mut game, soe_papyrus::Value::Object(alias));
    assert_eq!(messages.len(), 1, "{messages:?}");
    let msg = &messages[0];
    assert!(
        !msg.contains(':'),
        "an alias has no form id, trace message should not carry one, got {msg:?}"
    );
}

/// A `None` object argument still traces as an empty message, unaffected
/// by the form id change (`text` handles `Value::None` before `text_world`
/// ever looks for a form).
#[test]
fn scr_07_trace_none_argument_is_unaffected() {
    let mut game = game_with_marker();
    let messages = trace(&mut game, soe_papyrus::Value::None);
    assert_eq!(messages, vec![String::new()], "{messages:?}");
}
