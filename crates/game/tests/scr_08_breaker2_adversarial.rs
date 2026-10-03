// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Additional SCR-08 contract boundary checks.

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
    order.add("Scr08Breaker2.esm", plugin).unwrap();
    Game::new(order)
}

fn call_actor(game: &mut Game, method: &str, args: &[Value]) -> Value {
    let class = format!("Scr08Breaker2{method}");
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
fn scr_08_alarm_and_movement_queries_are_repeatable_reads() {
    let mut game = game();
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.alerted = true;
        actor.running = true;
        actor.sprinting = false;
    }
    for _ in 0..3 {
        assert_eq!(call_actor(&mut game, "IsAlarmed", &[]), Value::Bool(true));
        assert_eq!(call_actor(&mut game, "IsRunning", &[]), Value::Bool(true));
        assert_eq!(call_actor(&mut game, "IsSprinting", &[]), Value::Bool(false));
    }
    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    assert!(actor.alerted && actor.running && !actor.sprinting);
}

#[test]
fn scr_08_hostility_requires_the_supplied_actor_not_another_target() {
    let mut game = game();
    game.world_mut().actor(id(TARGET));
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.in_combat = true;
        actor.combat_target = Some(id(TARGET));
    }
    assert_eq!(
        call_actor(&mut game, "IsHostileToActor", &[Value::None]),
        Value::Bool(false)
    );
}
