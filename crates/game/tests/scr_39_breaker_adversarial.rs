// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial SCR-39 checks against the local Creation Kit contracts.

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
    order.add("Scr39Breaker.esp", plugin).unwrap();
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

fn call(game: &mut Game, reference: GlobalFormId, name: &str, args: &[Value]) -> Option<Value> {
    let script_name = format!(
        "Scr39BreakerProbe{}",
        PROBE_ID.fetch_add(1, Ordering::Relaxed)
    );
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
    let values = std::iter::once(Value::Object(game.object_of_form(reference)))
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    let thread = game.vm.call_global(&script_name, "Run", &values).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread)
}

#[test]
fn play_animation_and_wait_stays_latent_until_the_named_event() {
    let mut game = game();
    let result = call(
        &mut game,
        id(REFERENCE),
        "PlayAnimationAndWait",
        &[Value::string("Open"), Value::string("OpenDone")],
    );
    assert_eq!(result, None, "the CK native is latent until OpenDone fires");
}

#[test]
fn gamebryo_animation_does_not_emit_a_graph_animation_event() {
    let mut game = game();
    assert_eq!(
        call(
            &mut game,
            id(REFERENCE),
            "PlayGamebryoAnimation",
            &[
                Value::string("Forward"),
                Value::Bool(false),
                Value::Float(1.0)
            ],
        ),
        Some(Value::Bool(false))
    );
    assert!(
        game.world().animation_events.is_empty(),
        "legacy NIF playback is not an animation graph event"
    );
}

#[test]
fn current_scene_returns_none_for_a_nonparticipant_reference() {
    let mut game = game();
    assert_eq!(
        call(&mut game, id(REFERENCE), "GetCurrentScene", &[]),
        Some(Value::None)
    );
}

#[test]
fn motion_type_accepts_every_documented_havok_value() {
    let mut game = game();
    for motion_type in 1..=7 {
        assert_eq!(
            call(
                &mut game,
                id(REFERENCE),
                "SetMotionType",
                &[Value::Int(motion_type)],
            ),
            Some(Value::None)
        );
    }
    let requests = game.world_mut().take_reference_physics_requests();
    assert_eq!(requests.len(), 7);
    for (index, request) in requests.into_iter().enumerate() {
        assert_eq!(
            request,
            ReferencePhysicsRequest::MotionType {
                reference: id(REFERENCE),
                motion_type: (index + 1) as u32,
                allow_activate: true,
            }
        );
    }
}

#[test]
fn zero_magnitude_impulse_is_still_queued_for_havok_settling() {
    let mut game = game();
    assert_eq!(
        call(
            &mut game,
            id(REFERENCE),
            "ApplyHavokImpulse",
            &[
                Value::Float(0.0),
                Value::Float(0.0),
                Value::Float(1.0),
                Value::Float(0.0),
            ],
        ),
        Some(Value::None)
    );
    assert_eq!(
        game.world_mut().take_reference_physics_requests(),
        vec![ReferencePhysicsRequest::Impulse {
            reference: id(REFERENCE),
            direction: [0.0, 0.0, 1.0],
            magnitude: 0.0,
        }]
    );
}

#[test]
fn missing_reference_rejects_animation_and_physics_requests() {
    let mut game = game();
    let missing = id(0x999);
    assert_eq!(
        call(
            &mut game,
            missing,
            "PlayAnimationAndWait",
            &[Value::string("Open"), Value::string("OpenDone")],
        ),
        Some(Value::Bool(false))
    );
    assert_eq!(
        call(
            &mut game,
            missing,
            "PlayGamebryoAnimation",
            &[Value::string("Forward")],
        ),
        Some(Value::Bool(false))
    );
    call(&mut game, missing, "SetMotionType", &[Value::Int(4)]);
    call(
        &mut game,
        missing,
        "ApplyHavokImpulse",
        &[
            Value::Float(1.0),
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(5.0),
        ],
    );
    assert!(game.world().animation_events.is_empty());
    assert!(game
        .world_mut()
        .take_reference_physics_requests()
        .is_empty());
}
