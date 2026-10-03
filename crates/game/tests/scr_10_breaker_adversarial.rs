// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! SCR-10 checks for the documented Actor native contracts.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const ACTOR: u32 = 0x801;
const TARGET: u32 = 0x802;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        0x800,
        "Probe",
        "Probe",
        Config::default(),
        Stats::default(),
    ));
    for reference in [ACTOR, TARGET] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), reference).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Scr10Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn actor_call(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let class = format!("Scr10Contract{name}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("actor", "Actor")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("actor"), ident("result")];
    for (index, arg) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        function = function.param(
            &parameter,
            match arg {
                Value::Bool(_) => "Bool",
                Value::Float(_) => "Float",
                Value::Int(_) => "Int",
                Value::Str(_) => "String",
                _ => "ObjectReference",
            },
        );
        operands.push(ident(&parameter));
    }
    let bytes = ScriptBuilder::new(&class, "Form")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut arguments = vec![Value::Object(game.object_of_form(id(ACTOR)))];
    arguments.extend_from_slice(args);
    let thread = game.vm.call_global(&class, "Run", &arguments).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn scr_10_has_los_rejects_missing_target_argument() {
    let mut game = game();
    assert_eq!(actor_call(&mut game, "HasLOS", &[]), Value::Bool(false));
}

#[test]
fn scr_10_has_los_rejects_disabled_target() {
    let mut game = game();
    game.world_mut().reference(id(TARGET)).unwrap().enabled = false;
    let target = Value::Object(game.object_of_form(id(TARGET)));
    assert_eq!(
        actor_call(&mut game, "HasLOS", &[target]),
        Value::Bool(false)
    );
}

#[test]
fn scr_10_path_rejects_non_finite_walk_run_percentage() {
    let mut game = game();
    let target = Value::Object(game.object_of_form(id(TARGET)));
    for speed in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            actor_call(
                &mut game,
                "PathToReference",
                &[target.clone(), Value::Float(speed)]
            ),
            Value::Bool(false)
        );
    }
}

#[test]
fn scr_10_path_rejects_percentage_outside_inclusive_range() {
    let mut game = game();
    let target = Value::Object(game.object_of_form(id(TARGET)));
    for speed in [-f32::EPSILON, 1.0 + f32::EPSILON] {
        assert_eq!(
            actor_call(
                &mut game,
                "PathToReference",
                &[target.clone(), Value::Float(speed)]
            ),
            Value::Bool(false)
        );
    }
}

#[test]
fn scr_10_flying_state_only_accepts_documented_live_state_range() {
    let mut game = game();
    game.world_mut().drive_actor(id(ACTOR));
    for state in [0, 5, u8::MAX] {
        game.world_mut().actor(id(ACTOR)).unwrap().flying_state = Some(state);
        assert_eq!(actor_call(&mut game, "IsFlying", &[]), Value::Bool(false));
    }
    for state in 1..=4 {
        game.world_mut().actor(id(ACTOR)).unwrap().flying_state = Some(state);
        assert_eq!(actor_call(&mut game, "IsFlying", &[]), Value::Bool(true));
    }
}
