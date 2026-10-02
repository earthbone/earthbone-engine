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
const OTHER_REFERENCE: u32 = 0x804;
static PROBE_ID: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for (reference, base) in [(REFERENCE, 0x803_u32), (OTHER_REFERENCE, 0x805_u32)] {
        plugin.push_top(
            Record::new(Tag(*b"REFR"), reference)
                .sub(Tag(*b"NAME"), base.to_le_bytes())
                .sub(Tag(*b"DATA"), vec![0; 24]),
        );
        plugin.push_top(Record::new(Tag(*b"STAT"), base));
    }
    let mut order = LoadOrder::new();
    order.add("Scr39Adversarial.esp", plugin).unwrap();
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

fn start_call(
    game: &mut Game,
    reference: GlobalFormId,
    name: &str,
    args: &[Value],
) -> soe_papyrus::vm::ThreadId {
    let script_name = format!(
        "Scr39AdversarialProbe{}",
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
    let mut values = vec![Value::Object(game.object_of_form(reference))];
    values.extend_from_slice(args);
    game.vm.call_global(&script_name, "Run", &values).unwrap()
}

fn call(game: &mut Game, reference: GlobalFormId, name: &str, args: &[Value]) -> Option<Value> {
    let thread = start_call(game, reference, name, args);
    game.run_until_idle(64);
    game.vm.thread_result(thread)
}

#[test]
fn impulse_rejects_nonfinite_components_and_magnitude_without_partial_requests() {
    let mut game = game();
    for args in [
        [
            Value::Float(f32::NAN),
            Value::Float(0.0),
            Value::Float(1.0),
            Value::Float(2.0),
        ],
        [
            Value::Float(0.0),
            Value::Float(f32::INFINITY),
            Value::Float(1.0),
            Value::Float(2.0),
        ],
        [
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(1.0),
            Value::Float(f32::NAN),
        ],
    ] {
        assert_eq!(
            call(&mut game, id(REFERENCE), "ApplyHavokImpulse", &args),
            Some(Value::None)
        );
    }
    assert!(game
        .world_mut()
        .take_reference_physics_requests()
        .is_empty());
}

#[test]
fn zero_magnitude_impulse_is_permitted_and_each_call_queues_once() {
    let mut game = game();
    for _ in 0..2 {
        assert_eq!(
            call(
                &mut game,
                id(REFERENCE),
                "ApplyHavokImpulse",
                &[
                    Value::Float(0.0),
                    Value::Float(0.0),
                    Value::Float(1.0),
                    Value::Float(0.0)
                ],
            ),
            Some(Value::None)
        );
    }
    assert_eq!(
        game.world_mut().take_reference_physics_requests(),
        vec![
            ReferencePhysicsRequest::Impulse {
                reference: id(REFERENCE),
                direction: [0.0, 0.0, 1.0],
                magnitude: 0.0,
            };
            2
        ]
    );
}

#[test]
fn impulse_on_reference_without_resident_three_d_is_queued_for_host_filtering() {
    let mut game = game();
    assert_eq!(
        call(
            &mut game,
            id(REFERENCE),
            "ApplyHavokImpulse",
            &[
                Value::Float(1.0),
                Value::Float(0.0),
                Value::Float(0.0),
                Value::Float(5.0)
            ],
        ),
        Some(Value::None)
    );
    assert_eq!(
        game.world_mut().take_reference_physics_requests(),
        vec![ReferencePhysicsRequest::Impulse {
            reference: id(REFERENCE),
            direction: [1.0, 0.0, 0.0],
            magnitude: 5.0,
        }]
    );
}

#[test]
fn animation_wait_ignores_other_targets_and_aborts_when_its_visual_unloads() {
    let mut game = game();
    let thread = start_call(
        &mut game,
        id(REFERENCE),
        "PlayAnimationAndWait",
        &[Value::string("Open"), Value::string("OpenDone")],
    );
    game.run_until_idle(64);
    game.report_animation_event(id(OTHER_REFERENCE), "OpenDone");
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), None);
    game.report_animation_unloaded(id(REFERENCE));
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::Bool(false)));
}
