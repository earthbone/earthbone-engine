// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for SCR-07 (`Debug.Trace` carries an object
//! argument's form id). These target the id the change actually prints,
//! not just whether one is present.
//!
//! The done check requires the id shown to be the one "the game prints",
//! and this engine already has a function for that conversion:
//! `World::runtime_id`, used everywhere else a log line needs the vanilla
//! 8-hex-digit FormID (`crates/game/src/world/objects.rs`, the `object_of`
//! naming fallback: `format!("{:08X}", self.runtime_id(id))`). `text_world`
//! (`crates/game/src/natives.rs`) instead prints `GlobalFormId`'s own
//! `Display`, `{file}:{object:06X}`, which is the load order slot and the
//! raw in-file object id, not a FormID. The two agree only when a form's
//! load order file index equals its true runtime top byte, which is not
//! the general case: the player reference, a dynamically created form and
//! any form outside the first loaded master all diverge.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::world::{DYNAMIC_FILE, PLAYER_OBJECT};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, i, ident, s};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");
const MARKER: u32 = 0x800;
const DLC_FORM: u32 = 0xA01;

/// One master plugin with a single `STAT` "Marker", loaded and started.
fn game_with_marker() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"STAT"), MARKER).zsub(EDID, "Marker"));
    let mut order = LoadOrder::new();
    order.add("Test.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.start();
    game
}

/// Two master plugins, so a form in the second one sits at load order slot
/// 1, the shape every install with an official DLC or another mod has.
fn game_with_second_master_form() -> Game {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(T(b"STAT"), MARKER).zsub(EDID, "Marker"));
    let mut dlc = Plugin::new(FileHeader::default());
    dlc.push_top(Record::new(T(b"STAT"), DLC_FORM).zsub(EDID, "DlcMarker"));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Dlc.esm", dlc).unwrap();
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
            vec![s("Debug"), s("Trace"), ident("::nonevar"), ident("t"), i(0)],
        )
        .build();
    ScriptBuilder::new("TraceProbe", "Form")
        .function(run)
        .bytes()
}

/// Runs `TraceProbe.Run(target)` to completion and hands back the papyrus
/// log's messages, in order.
fn trace(game: &mut Game, target: Value) -> Vec<String> {
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

/// The player reference is form id `0x00000014`, not a load-order-relative
/// id: `CommonLibSSE-NG/include/RE/T/TESForm.h` line 344 defines
/// `IsPlayerRef()` as `GetFormID() == 0x00000014`, and this engine's own
/// `World::runtime_id` (`crates/game/src/world/lookup.rs`) special-cases
/// `file == 0 && object == PLAYER_OBJECT` to return that same `0x14`
/// verbatim, exactly because the load-order slot is not the id the game
/// shows for it. `text_world` instead prints `GlobalFormId`'s raw
/// `{file}:{object:06X}`, so tracing the player reference should show
/// `00000014` and must not show the load-order-slot form `0:000014`.
#[test]
fn scr_07_player_reference_traces_the_real_form_id() {
    let mut game = game_with_marker();
    let player = game.object_of_form(GlobalFormId {
        file: 0,
        object: PLAYER_OBJECT,
    });
    let messages = trace(&mut game, Value::Object(player));
    assert_eq!(messages.len(), 1, "{messages:?}");
    let msg = &messages[0];
    assert!(
        msg.contains("00000014"),
        "trace of the player reference should carry its real form id 00000014, got {msg:?}"
    );
    assert!(
        !msg.contains("0:000014"),
        "trace should not show the load-order-slot notation for the player, got {msg:?}"
    );
}

/// A form created at runtime (no plugin owns it) keeps a `DYNAMIC_FILE`
/// sentinel in its `GlobalFormId`, but `world.rs`'s own doc comment above
/// `DYNAMIC_FILE` states: "Runtime FormIDs still use the FF prefix."
/// `World::runtime_id`/`LoadOrder::runtime_id` (`crates/esm/src/load_order.rs`
/// line 196) implement exactly that: `0xFF00_0000 | (id.object & 0x00FF_FFFF)`.
/// `text_world` instead prints the sentinel's raw numeric value
/// (`u16::MAX`, i.e. `65535`), never the `FF` prefix the engine's own
/// comment promises.
#[test]
fn scr_07_dynamic_form_traces_with_ff_prefix() {
    let mut game = game_with_marker();
    let dynamic = game.object_of_form(GlobalFormId {
        file: DYNAMIC_FILE,
        object: 0x000001,
    });
    let messages = trace(&mut game, Value::Object(dynamic));
    assert_eq!(messages.len(), 1, "{messages:?}");
    let msg = &messages[0];
    assert!(
        msg.contains("FF000001"),
        "a runtime-created form should trace with the FF prefix the engine's \
         own DYNAMIC_FILE comment promises, got {msg:?}"
    );
    assert!(
        !msg.contains("65535"),
        "trace should never leak the DYNAMIC_FILE sentinel value, got {msg:?}"
    );
}

/// `Game::save_state`/`load_state` (`crates/game/src/save.rs`) replace the
/// world's object table wholesale, so an `ObjectId` allocated in the
/// restored game need not equal the one the original game had for the same
/// form. `text_world` converts through `World::form_of` on the handle
/// behind that id, not through the id's numeric value, so the printed form
/// id should be unaffected by a save/load round trip even if the
/// allocation underneath it changed.
#[test]
fn scr_07_form_id_survives_a_save_and_load_round_trip() {
    let original = game_with_marker();
    let marker = original.form("Marker").expect("Marker form present");
    let saved = original.save_state().expect("game state saves");

    let mut restored = game_with_marker();
    restored.load_state(&saved).expect("game state loads");

    let object = restored.object_of_form(marker);
    let messages = trace(&mut restored, Value::Object(object));
    assert_eq!(messages.len(), 1, "{messages:?}");
    let msg = &messages[0];
    assert!(
        msg.contains("00000800"),
        "a form traced after a save/load round trip should still carry its \
         real runtime form id 00000800, not a stale or reallocated one, got {msg:?}"
    );
}

/// A form owned by the second loaded master sits at load order slot 1, the
/// layout every install with Dawnguard, HearthFires or Dragonborn (or any
/// second mod) has. `LoadOrder::runtime_id` (`crates/esm/src/load_order.rs`
/// line 199) shifts the slot into the top byte: `(u32::from(s) << 24) |
/// (id.object & 0x00FF_FFFF)`, so `DlcMarker` (raw object `0xA01`) is real
/// form id `0x0100_0A01`. `text_world`'s `{file}:{object:06X}` instead
/// prints the load order file index as a decimal prefix, which is not a
/// form id the game or any tool (xEdit, the CK, PapyrusUtil) would print.
#[test]
fn scr_07_second_master_form_traces_the_shifted_runtime_id() {
    let mut game = game_with_second_master_form();
    let dlc_form = game.form("DlcMarker").expect("DlcMarker form present");
    let object = game.object_of_form(dlc_form);
    let messages = trace(&mut game, Value::Object(object));
    assert_eq!(messages.len(), 1, "{messages:?}");
    let msg = &messages[0];
    assert!(
        msg.contains("01000A01"),
        "a form from the second master should trace with its shifted \
         runtime FormID 01000A01, got {msg:?}"
    );
    assert!(
        !msg.contains("1:000A01"),
        "trace should not show the raw load-order-slot notation for a DLC form, got {msg:?}"
    );
}
