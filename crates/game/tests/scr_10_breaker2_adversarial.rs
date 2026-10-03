// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Additional SCR-10 source contract checks.

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
    order.add("Scr10Breaker2.esm", plugin).unwrap();
    Game::new(order)
}

fn actor_call(game: &mut Game, method: &str, args: &[Value]) -> Value {
    let class = format!("Scr10Breaker2{method}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("actor", "Actor")
        .local("result", "Var");
    let mut operands = vec![s(method), ident("actor"), ident("result")];
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
fn scr_10_alerted_without_combat_is_not_the_searching_combat_state() {
    let mut game = game();
    game.world_mut().actor(id(ACTOR)).unwrap().alerted = true;
    assert_eq!(actor_call(&mut game, "GetCombatState", &[]), Value::Int(0));
}

#[test]
fn scr_10_path_to_unreached_target_suspends_until_path_completion() {
    let mut game = game();
    let target = Value::Object(game.object_of_form(id(TARGET)));
    let class = "Scr10Breaker2LatentPath";
    let function = FunctionBuilder::new("Run", "Bool")
        .global()
        .param("actor", "Actor")
        .param("target", "ObjectReference")
        .param("speed", "Float")
        .local("result", "Bool")
        .ins(
            CallMethod,
            vec![
                s("PathToReference"),
                ident("actor"),
                ident("target"),
                ident("speed"),
                ident("result"),
            ],
        )
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(&ScriptBuilder::new(class, "Form").function(function).bytes())
        .unwrap();
    let actor = Value::Object(game.object_of_form(id(ACTOR)));
    let thread = game
        .vm
        .call_global(class, "Run", &[actor, target, Value::Float(0.5)])
        .unwrap();
    assert!(game.run_until_idle(64));
    assert!(game.vm.thread_result(thread).is_none());
}
