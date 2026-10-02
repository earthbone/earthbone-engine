// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! SCR-08 adversarial Actor native contract cases.

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
        Config {
            flags: npc::flags::BLEEDOUT_OVERRIDE | npc::flags::PROTECTED,
            bleedout: 20,
            ..Config::default()
        },
        Stats::default(),
    ));
    plugin.push_top(Record::new(Tag(*b"ACHR"), ACTOR).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ACHR"), TARGET).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Scr08Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn actor_bool(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let class = format!("Scr08Breaker{name}");
    let mut function = FunctionBuilder::new("Run", "Bool")
        .global()
        .param("actor", "Actor")
        .local("result", "Bool");
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
    let object = game.object_of_form(id(ACTOR));
    let mut arguments = vec![Value::Object(object)];
    arguments.extend_from_slice(args);
    let thread = game.vm.call_global(&class, "Run", &arguments).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn scr_08_breaker_running_is_false_while_sprinting() {
    let mut game = game();
    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    actor.running = true;
    actor.sprinting = true;
    drop(world);
    assert_eq!(actor_bool(&mut game, "IsRunning", &[]), Value::Bool(false));
}

#[test]
fn scr_08_breaker_bleedout_threshold_is_inclusive() {
    let mut game = game();
    let actor = id(ACTOR);
    game.world_mut().actor(actor).unwrap().unconscious = true;
    game.world_mut().set_actor_value(actor, "health", 20.01).unwrap();
    assert_eq!(actor_bool(&mut game, "IsBleedingOut", &[]), Value::Bool(false));
    game.world_mut().set_actor_value(actor, "health", 20.0).unwrap();
    assert_eq!(actor_bool(&mut game, "IsBleedingOut", &[]), Value::Bool(true));
}

#[test]
fn scr_08_breaker_missing_actor_has_quiet_false_for_each_query() {
    let mut game = game();
    let missing = Value::Object(game.object_of_form(id(0x8ff)));
    for method in ["IsAlarmed", "IsBleedingOut", "IsRunning", "IsSprinting"] {
        let class = format!("Scr08Missing{method}");
        let function = FunctionBuilder::new("Run", "Bool")
            .global()
            .param("actor", "Actor")
            .local("result", "Bool")
            .ins(CallMethod, vec![s(method), ident("actor"), ident("result")])
            .ins(Return, vec![ident("result")])
            .build();
        game.load_script(&ScriptBuilder::new(&class, "Form").function(function).bytes())
            .unwrap();
        let thread = game.vm.call_global(&class, "Run", &[missing.clone()]).unwrap();
        assert!(game.run_until_idle(64));
        assert_eq!(game.vm.thread_result(thread).unwrap(), Value::Bool(false), "{method}");
    }
}

#[test]
fn scr_08_breaker_hostility_is_specific_to_the_argument_actor() {
    let mut game = game();
    let actor = id(ACTOR);
    let target = id(TARGET);
    let other = id(0x803);
    game.world_mut().actor(target);
    game.world_mut().actor(other);
    {
        let mut world = game.world_mut();
        let state = world.actor(actor).unwrap();
        state.in_combat = true;
        state.combat_target = Some(target);
    }
    let other_object = Value::Object(game.object_of_form(other));
    assert_eq!(actor_bool(&mut game, "IsHostileToActor", &[other_object]), Value::Bool(false));
}

#[test]
fn scr_08_breaker_hostility_with_null_argument_returns_false() {
    let mut game = game();
    assert_eq!(actor_bool(&mut game, "IsHostileToActor", &[Value::None]), Value::Bool(false));
}
