// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! SCR-10 source-backed Actor query edge cases.

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
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x803).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Scr10Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn actor_call(game: &mut Game, name: &str, args: &[Value]) -> Value {
    actor_call_on(game, id(ACTOR), name, args)
}

fn actor_call_on(game: &mut Game, actor_form: GlobalFormId, name: &str, args: &[Value]) -> Value {
    let class = format!("Scr10Breaker{name}");
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
    let object = game.object_of_form(actor_form);
    let mut arguments = vec![Value::Object(object)];
    arguments.extend_from_slice(args);
    let thread = game.vm.call_global(&class, "Run", &arguments).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn scr_10_alert_does_not_report_searching_without_combat_state() {
    let mut game = game();
    game.world_mut().drive_actor(id(ACTOR));
    assert_eq!(actor_call(&mut game, "GetCombatState", &[]), Value::Int(0));
    game.world_mut().actor(id(ACTOR)).unwrap().in_combat = true;
    assert_eq!(actor_call(&mut game, "GetCombatState", &[]), Value::Int(1));
    game.world_mut().actor(id(ACTOR)).unwrap().in_combat = false;
    game.world_mut().actor(id(ACTOR)).unwrap().alerted = true;
    assert_eq!(actor_call(&mut game, "GetCombatState", &[]), Value::Int(0));
}

#[test]
fn scr_10_sleep_query_distinguishes_transitional_wants_to_sleep_state() {
    let mut game = game();
    game.world_mut().drive_actor(id(ACTOR));
    game.world_mut().actor(id(ACTOR)).unwrap().ai.sleeping = true;
    assert_eq!(actor_call(&mut game, "GetSleepState", &[]), Value::Int(3));
    game.world_mut().actor(id(ACTOR)).unwrap().ai.procedure = soe_game::ai::Procedure::Travel;
    assert_eq!(actor_call(&mut game, "GetSleepState", &[]), Value::Int(3));
    game.world_mut().actor(id(ACTOR)).unwrap().ai.sleeping = false;
    assert_eq!(actor_call(&mut game, "GetSleepState", &[]), Value::Int(0));
    game.world_mut().actor(id(ACTOR)).unwrap().ai.procedure = soe_game::ai::Procedure::Sleep;
    assert_eq!(actor_call(&mut game, "GetSleepState", &[]), Value::Int(2));
    game.world_mut().actor(id(ACTOR)).unwrap().ai.procedure = soe_game::ai::Procedure::None;
    assert_eq!(actor_call(&mut game, "GetSleepState", &[]), Value::Int(0));
}

#[test]
fn scr_10_valid_path_request_to_coincident_target_completes() {
    let mut game = game();
    let target = Value::Object(game.object_of_form(id(TARGET)));
    assert_eq!(
        actor_call(&mut game, "PathToReference", &[target, Value::Float(0.5)]),
        Value::Bool(true)
    );
}

#[test]
fn scr_10_same_position_actor_has_line_of_sight_to_actor() {
    let mut game = game();
    let target = Value::Object(game.object_of_form(id(TARGET)));
    assert_eq!(
        actor_call(&mut game, "HasLOS", &[target]),
        Value::Bool(true)
    );
}

#[test]
fn scr_10_actor_line_of_sight_accepts_distinct_positions_in_same_space() {
    let mut game = game();
    game.world_mut().reference(id(ACTOR)).unwrap().position = [10.0, 0.0, 0.0];
    game.world_mut().reference(id(TARGET)).unwrap().position = [20.0, 0.0, 0.0];
    let target = Value::Object(game.object_of_form(id(TARGET)));
    assert_eq!(
        actor_call(&mut game, "HasLOS", &[target]),
        Value::Bool(true)
    );
}

#[test]
fn scr_10_non_player_cannot_get_los_to_non_actor_reference() {
    let mut game = game();
    let target = Value::Object(game.object_of_form(id(0x803)));
    assert_eq!(
        actor_call(&mut game, "HasLOS", &[target]),
        Value::Bool(false)
    );
}

#[test]
fn scr_10_player_can_query_los_to_non_actor_reference() {
    let mut game = game();
    let player = id(0x14);
    assert_eq!(game.player(), player);
    game.world_mut().reference(player).unwrap().position = [10.0, 0.0, 0.0];
    game.world_mut().reference(id(0x803)).unwrap().position = [10.0, 0.0, 0.0];
    let target = Value::Object(game.object_of_form(id(0x803)));
    assert_eq!(
        actor_call_on(&mut game, player, "HasLOS", &[target]),
        Value::Bool(true)
    );
}

#[test]
fn scr_10_path_arguments_fail_for_invalid_speed_and_missing_target() {
    let mut game = game();
    assert_eq!(
        actor_call(
            &mut game,
            "PathToReference",
            &[Value::None, Value::Float(1.01)]
        ),
        Value::Bool(false)
    );
    assert_eq!(
        actor_call(
            &mut game,
            "PathToReference",
            &[Value::None, Value::Float(0.5)]
        ),
        Value::Bool(false)
    );
    let target = Value::Object(game.object_of_form(id(TARGET)));
    for speed in [0.0, 1.0] {
        assert_eq!(
            actor_call(
                &mut game,
                "PathToReference",
                &[target.clone(), Value::Float(speed)]
            ),
            Value::Bool(true)
        );
    }
}

#[test]
fn scr_10_is_flying_reads_live_actor_flight_state() {
    let mut game = game();
    game.world_mut().drive_actor(id(ACTOR));
    assert_eq!(actor_call(&mut game, "IsFlying", &[]), Value::Bool(false));
    game.world_mut().actor(id(ACTOR)).unwrap().flying = true;
    assert_eq!(actor_call(&mut game, "IsFlying", &[]), Value::Bool(true));
    game.world_mut().actor(id(ACTOR)).unwrap().flying = false;
    game.world_mut().actor(id(ACTOR)).unwrap().flying_state = Some(2);
    assert_eq!(actor_call(&mut game, "IsFlying", &[]), Value::Bool(true));
}
