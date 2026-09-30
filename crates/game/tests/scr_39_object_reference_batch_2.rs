// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-39 contracts for the ObjectReference native batch.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::ReferencePhysicsRequest;
use soe_game::Game;
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
    order.add("Scr39.esp", plugin).unwrap();
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

fn call(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script_name = format!("Scr39Probe{}", PROBE_ID.fetch_add(1, Ordering::Relaxed));
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
    let thread = game.vm.call_global(&script_name, "Run", &values).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_39_animation_and_wait_records_animation_and_rejects_empty_arguments() {
    let mut game = game();
    let result = call(
        &mut game,
        "PlayAnimationAndWait",
        &[Value::string("Open"), Value::string("OpenDone")],
    );
    assert_eq!(result, Value::Bool(true));
    assert_eq!(
        game.world().animation_events,
        vec![(id(REFERENCE), "Open".to_owned())]
    );
    let rejected = call(
        &mut game,
        "PlayAnimationAndWait",
        &[Value::string(""), Value::string("OpenDone")],
    );
    assert_eq!(rejected, Value::Bool(false));
    assert_eq!(game.world().animation_events.len(), 1);
}

#[test]
fn scr_39_gamebryo_animation_records_the_named_animation() {
    let mut game = game();
    let result = call(
        &mut game,
        "PlayGamebryoAnimation",
        &[
            Value::string("Forward"),
            Value::Bool(true),
            Value::Float(0.5),
        ],
    );
    assert_eq!(result, Value::Bool(true));
    assert_eq!(
        game.world().animation_events,
        vec![(id(REFERENCE), "Forward".to_owned())]
    );
    assert_eq!(
        call(&mut game, "PlayGamebryoAnimation", &[Value::string("")]),
        Value::Bool(false)
    );
}

#[test]
fn scr_39_motion_type_preserves_type_and_activation_arguments() {
    let mut game = game();
    assert_eq!(
        call(
            &mut game,
            "SetMotionType",
            &[Value::Int(4), Value::Bool(false)],
        ),
        Value::None
    );
    assert_eq!(
        game.world_mut().take_reference_physics_requests(),
        vec![ReferencePhysicsRequest::MotionType {
            reference: id(REFERENCE),
            motion_type: 4,
            allow_activate: false,
        }]
    );
    call(&mut game, "SetMotionType", &[Value::Int(8)]);
    assert!(game
        .world_mut()
        .take_reference_physics_requests()
        .is_empty());
}

#[test]
fn scr_39_havok_impulse_normalizes_direction_and_rejects_zero_vector() {
    let mut game = game();
    call(
        &mut game,
        "ApplyHavokImpulse",
        &[
            Value::Float(3.0),
            Value::Float(4.0),
            Value::Float(0.0),
            Value::Float(5.0),
        ],
    );
    assert_eq!(
        game.world_mut().take_reference_physics_requests(),
        vec![ReferencePhysicsRequest::Impulse {
            reference: id(REFERENCE),
            direction: [0.6, 0.8, 0.0],
            magnitude: 5.0,
        }]
    );
    call(
        &mut game,
        "ApplyHavokImpulse",
        &[
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(5.0),
        ],
    );
    assert!(game
        .world_mut()
        .take_reference_physics_requests()
        .is_empty());
}
