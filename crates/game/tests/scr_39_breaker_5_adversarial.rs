// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

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
    order.add("Scr39Breaker5.esp", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script_name = format!(
        "Scr39Breaker5Probe{}",
        PROBE_ID.fetch_add(1, Ordering::Relaxed)
    );
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("reference", "ObjectReference")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("reference"), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let param = format!("arg{index}");
        function = function.param(
            &param,
            match value {
                Value::Int(_) => "Int",
                Value::Float(_) => "Float",
                Value::Bool(_) => "Bool",
                _ => "Var",
            },
        );
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
fn scr_39_zero_magnitude_havok_impulse_is_still_sent_to_physics() {
    let mut game = game();
    assert_eq!(
        call(
            &mut game,
            "ApplyHavokImpulse",
            &[
                Value::Float(0.0),
                Value::Float(0.0),
                Value::Float(1.0),
                Value::Float(0.0),
            ],
        ),
        Value::None
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
