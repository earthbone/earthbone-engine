// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-39 checks against Creation Kit ObjectReference contracts.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::ReferencePhysicsRequest, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

const REFERENCE: u32 = 0x802;
static PROBE_ID: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"REFR"), REFERENCE)
            .sub(Tag(*b"NAME"), 0x803_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x803));
    let mut order = LoadOrder::new();
    order.add("Scr39Breaker2.esp", plugin).unwrap();
    Game::new(order)
}

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::Bool(_) => "Bool",
        Value::Str(_) => "String",
        _ => "Form",
    }
}

fn start_call(game: &mut Game, name: &str, args: &[Value]) -> soe_papyrus::vm::ThreadId {
    let script_name = format!("Scr39Breaker2Probe{}", PROBE_ID.fetch_add(1, Ordering::Relaxed));
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("reference", "ObjectReference")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("reference"), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let param = format!("arg{index}");
        function = function.param(&param, type_of(value));
        operands.push(ident(&param));
    }
    let bytes = ScriptBuilder::new(&script_name, "Form")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut values = vec![Value::Object(game.object_of_form(id(REFERENCE)))];
    values.extend_from_slice(args);
    game.vm.call_global(&script_name, "Run", &values).unwrap()
}

fn call(game: &mut Game, name: &str, args: &[Value]) -> Option<Value> {
    let thread = start_call(game, name, args);
    game.run_until_idle(64);
    game.vm.thread_result(thread)
}

#[test]
fn play_animation_and_wait_returns_only_after_the_named_graph_event() {
    let mut game = game();
    let thread = start_call(
        &mut game,
        "PlayAnimationAndWait",
        &[Value::string("Open"), Value::string("OpenDone")],
    );
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), None, "latent native must wait for OpenDone");
    game.report_animation_event(id(REFERENCE), "OtherEvent");
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), None, "a different event must not release the wait");
    game.report_animation_event(id(REFERENCE), "OpenDone");
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::Bool(true)));
}

#[test]
fn gamebryo_animation_does_not_report_a_graph_animation_event() {
    let mut game = game();
    assert_eq!(
        call(
            &mut game,
            "PlayGamebryoAnimation",
            &[Value::string("Forward"), Value::Bool(true), Value::Float(0.75)],
        ),
        Some(Value::Bool(true))
    );
    assert!(game.world().animation_events.is_empty());
}

#[test]
fn finite_large_impulse_direction_is_normalized_and_zero_magnitude_is_valid() {
    let mut game = game();
    assert_eq!(
        call(
            &mut game,
            "ApplyHavokImpulse",
            &[
                Value::Float(1.0e20),
                Value::Float(0.0),
                Value::Float(0.0),
                Value::Float(0.0),
            ],
        ),
        Some(Value::None)
    );
    assert_eq!(
        game.world_mut().take_reference_physics_requests(),
        vec![ReferencePhysicsRequest::Impulse {
            reference: id(REFERENCE),
            direction: [1.0, 0.0, 0.0],
            magnitude: 0.0,
        }]
    );
}

#[test]
fn scene_none_and_character_motion_type_keep_their_documented_contracts() {
    let mut game = game();
    assert_eq!(call(&mut game, "GetCurrentScene", &[]), Some(Value::None));
    assert_eq!(
        call(&mut game, "SetMotionType", &[Value::Int(7), Value::Bool(false)]),
        Some(Value::None)
    );
    assert_eq!(
        game.world_mut().take_reference_physics_requests(),
        vec![ReferencePhysicsRequest::MotionType {
            reference: id(REFERENCE),
            motion_type: 7,
            allow_activate: false,
        }]
    );
}
