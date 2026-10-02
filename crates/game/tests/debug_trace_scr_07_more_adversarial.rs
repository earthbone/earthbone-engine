// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Further adversarial coverage for SCR-07 (`Debug.Trace` carries an
//! object argument's form id, `crates/game/src/natives.rs`, `text_world`).
//!
//! The existing `debug_trace_scr_07_adversarial.rs` covers the player
//! reference, a dynamically created form, a save and load round trip and a
//! form from a second regular master. This file attacks three cases the
//! task doc's own open questions raised but no existing test exercises:
//! a light (`ESL`) master's 12-bit slot encoding, `Handle::ActiveEffect`
//! (the sibling of `Handle::Alias` that also has no form id, per
//! `World::form_of`, `crates/game/src/world/objects.rs` line 64), and an
//! array of objects, the other shape `Debug.Trace` accepts through
//! Papyrus's implicit `asString` conversion besides a bare object.

use soe_esm::plugin::flags::LIGHT_MASTER;
use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::EffectId;
use soe_game::{Game, Handle};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};
use soe_papyrus::value::{ArrayRef, ElementKind};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");
const MARKER: u32 = 0x800;
const LIGHT_FORM: u32 = 0x005;

/// One regular master (`Base.esm`) so the game starts normally, plus a
/// second plugin with the `ESL` header flag set, which the load order
/// assigns a light slot instead of a regular one
/// (`crates/esm/src/load_order.rs` line 127).
fn game_with_light_master_form() -> Game {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(T(b"STAT"), MARKER).zsub(EDID, "Marker"));
    let mut light_header = FileHeader::default();
    light_header.flags |= LIGHT_MASTER;
    let mut light = Plugin::new(light_header);
    light.push_top(Record::new(T(b"STAT"), LIGHT_FORM).zsub(EDID, "LightMarker"));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Light.esl", light).unwrap();
    let mut game = Game::new(order);
    game.start();
    game
}

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

/// The light plugin sits at load order position 1, so
/// `LoadOrder::runtime_id` (`crates/esm/src/load_order.rs` line 200) should
/// print it as `0xFE00_0000 | (0 << 12) | 0x005`, i.e. `FE000005`, the
/// `FE`-prefixed 12-bit slot notation every ESL-aware tool (xEdit, the CK,
/// PapyrusUtil) uses, not the regular-master 8-bit slot notation the base
/// game would use for the same load order position (`01000005`).
#[test]
fn scr_07_light_master_form_traces_the_fe_prefixed_slot_id() {
    let mut game = game_with_light_master_form();
    let light_form = game.form("LightMarker").expect("LightMarker form present");
    let object = game.object_of_form(light_form);
    let messages = trace(&mut game, Value::Object(object));
    assert_eq!(messages.len(), 1, "{messages:?}");
    let msg = &messages[0];
    assert!(
        msg.contains("FE000005"),
        "a light master's form should trace with the FE-prefixed slot id FE000005, got {msg:?}"
    );
    assert!(
        !msg.contains("01000005"),
        "trace should not show the regular-master slot notation for a light master form, got {msg:?}"
    );
}

/// `Handle::ActiveEffect` is `form_of`'s other `None` case besides
/// `Handle::Alias` (`crates/game/src/world/objects.rs` line 64), called
/// out by name in the task doc's own open question 2 ("Aliases and
/// effects"), but the existing adversarial and conformance suites only
/// ever construct a `Handle::Alias`. An active effect handle needs no
/// running spell: `World::form_of` only matches on the handle's variant,
/// so this constructs one directly the way `crates/game/tests/magic.rs`
/// does for its own object-identity assertions.
#[test]
fn scr_07_trace_active_effect_receiver_falls_back_without_form_id() {
    let mut game = game_with_marker();
    let object = game.object_of(Handle::ActiveEffect(EffectId(1)));
    let messages = trace(&mut game, Value::Object(object));
    assert_eq!(messages.len(), 1, "{messages:?}");
    let msg = &messages[0];
    assert!(
        !msg.contains(':'),
        "an active effect has no form id, trace message should not carry one, got {msg:?}"
    );
    assert_eq!(
        msg,
        &Value::Object(object).to_string(),
        "an active effect should fall back to the bare object display, got {msg:?}"
    );
}

/// `Debug.Trace` accepts any type through Papyrus's implicit `asString`
/// conversion, including an `Object[]` array, not just a bare object. This
/// exercises whether `text_world`'s form id conversion reaches array
/// elements, or only a top-level object argument. `text` (called for the
/// fallback branch of `text_world`, `crates/game/src/natives.rs` line 70)
/// defers to `Value`'s own `Display`, which formats an array by printing
/// each element through the same bare, form-id-free `Display`
/// (`crates/papyrus/src/value.rs` lines 218-228), so this array of one
/// object is expected to trace WITHOUT the element's form id, unlike the
/// same object traced directly (`scr_07_trace_object_argument_carries_form_id`
/// in `debug_trace.rs`). This documents that gap rather than assuming it
/// away: a script that logs an object array for debugging (a common
/// PapyrusUtil pattern) gets none of SCR-07's benefit.
#[test]
fn scr_07_trace_object_array_does_not_carry_element_form_ids() {
    let mut game = game_with_marker();
    let marker = game.form("Marker").expect("Marker form present");
    let object = game.object_of_form(marker);

    let array = ArrayRef::new(ElementKind::Object, 1);
    array.items.borrow_mut()[0] = Value::Object(object);

    let messages = trace(&mut game, Value::Array(array));
    assert_eq!(messages.len(), 1, "{messages:?}");
    let msg = &messages[0];
    assert_eq!(
        msg,
        &format!("[{}]", Value::Object(object)),
        "an object array should trace each element with the bare object \
         display Debug.Trace on a single object no longer uses, got {msg:?}"
    );
    assert!(
        !msg.contains("00000800"),
        "if this fails, text_world now reaches into arrays too and this \
         test's documented gap has closed, got {msg:?}"
    );
}
