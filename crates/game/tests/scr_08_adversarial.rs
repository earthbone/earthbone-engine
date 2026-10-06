// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! SCR-08 source-backed actor state contract cases.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const ACTOR: u32 = 0x801;

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
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x802).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
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
fn scr_08_alarm_query_tracks_set_alert_both_directions() {
    let mut game = game();
    let actor = id(ACTOR);
    game.world_mut().actor(actor).unwrap().alerted = true;
    assert_eq!(actor_bool(&mut game, "IsAlarmed", &[]), Value::Bool(true));
    game.world_mut().actor(actor).unwrap().alerted = false;
    assert_eq!(actor_bool(&mut game, "IsAlarmed", &[]), Value::Bool(false));
}

#[test]
fn scr_08_bleeding_out_is_distinct_from_dead_or_ordinary_unconscious_state() {
    let mut game = game();
    let actor = id(ACTOR);
    game.world_mut().actor(actor).unwrap().dead = true;
    assert_eq!(
        actor_bool(&mut game, "IsBleedingOut", &[]),
        Value::Bool(false)
    );
    game.world_mut().actor(actor).unwrap().dead = false;
    game.world_mut()
        .set_actor_value(actor, "health", 100.0)
        .unwrap();
    game.world_mut().actor(actor).unwrap().unconscious = true;
    assert_eq!(
        actor_bool(&mut game, "IsBleedingOut", &[]),
        Value::Bool(false)
    );
    game.world_mut().actor(actor).unwrap().unconscious = false;
    game.world_mut()
        .damage_actor_value(actor, "health", 100.0)
        .unwrap();
    assert_eq!(
        actor_bool(&mut game, "IsBleedingOut", &[]),
        Value::Bool(true)
    );
}

#[test]
fn scr_08_running_query_reads_movement_state() {
    let mut game = game();
    let actor = id(ACTOR);
    assert_eq!(actor_bool(&mut game, "IsRunning", &[]), Value::Bool(false));
    game.world_mut().actor(actor).unwrap().running = true;
    assert_eq!(actor_bool(&mut game, "IsRunning", &[]), Value::Bool(true));
    game.world_mut().actor(actor).unwrap().running = false;
    assert_eq!(actor_bool(&mut game, "IsRunning", &[]), Value::Bool(false));
}

#[test]
fn scr_08_sprinting_query_reads_movement_state() {
    let mut game = game();
    let actor = id(ACTOR);
    assert_eq!(
        actor_bool(&mut game, "IsSprinting", &[]),
        Value::Bool(false)
    );
    game.world_mut().actor(actor).unwrap().sprinting = true;
    assert_eq!(actor_bool(&mut game, "IsSprinting", &[]), Value::Bool(true));
    game.world_mut().actor(actor).unwrap().sprinting = false;
    assert_eq!(
        actor_bool(&mut game, "IsSprinting", &[]),
        Value::Bool(false)
    );
}

#[test]
fn scr_08_hostility_query_uses_actor_argument_and_active_combat_target() {
    let mut game = game();
    let actor = id(ACTOR);
    let target = id(0x802);
    game.world_mut().actor(target);
    let target_object = Value::Object(game.object_of_form(target));

    assert_eq!(
        actor_bool(
            &mut game,
            "IsHostileToActor",
            std::slice::from_ref(&target_object)
        ),
        Value::Bool(false)
    );
    {
        let mut world = game.world_mut();
        let state = world.actor(actor).unwrap();
        state.in_combat = true;
        state.combat_target = Some(target);
    }
    assert_eq!(
        actor_bool(
            &mut game,
            "IsHostileToActor",
            std::slice::from_ref(&target_object)
        ),
        Value::Bool(true)
    );
    assert_eq!(
        actor_bool(&mut game, "IsHostileToActor", &[Value::Int(0)]),
        Value::Bool(false)
    );
    game.world_mut().actor(actor).unwrap().in_combat = false;
    assert_eq!(
        actor_bool(&mut game, "IsHostileToActor", &[target_object]),
        Value::Bool(false)
    );
}
