// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
static PROBES: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        BASE,
        "ProbeBase",
        "Probe Base",
        Config::default(),
        Stats::default(),
    ));
    let mut transform = vec![0; 24];
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), std::mem::take(&mut transform)),
    );
    let mut order = LoadOrder::new();
    order
        .add("Scr10.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Float(_) => "Float",
        Value::Int(_) => "Int",
        Value::Bool(_) => "Bool",
        Value::Str(_) => "String",
        _ => "ObjectReference",
    }
}

fn call(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let index = PROBES.fetch_add(1, Ordering::SeqCst);
    let script = format!("Scr10Probe{index}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("actor", "Actor")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("actor"), ident("result")];
    for (index, argument) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        function = function.param(&parameter, type_of(argument));
        operands.push(ident(&parameter));
    }
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let actor = game.object_of_form(id(ACTOR));
    let mut arguments = vec![Value::Object(actor)];
    arguments.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &arguments).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn scr_10_get_combat_state_reads_actor_combat_membership() {
    let mut game = game();
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    assert_eq!(call(&mut game, "GetCombatState", &[]), Value::Int(0));
    game.world_mut().actor(id(ACTOR)).unwrap().in_combat = true;
    assert_eq!(call(&mut game, "GetCombatState", &[]), Value::Int(1));
}

#[test]
fn scr_10_get_sleep_state_reads_actor_sleep_procedure() {
    let mut game = game();
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    assert_eq!(call(&mut game, "GetSleepState", &[]), Value::Int(0));
    game.world_mut().actor(id(ACTOR)).unwrap().ai.sleeping = true;
    assert_eq!(call(&mut game, "GetSleepState", &[]), Value::Int(3));
}

#[test]
fn scr_10_unavailable_los_path_and_flight_bindings_fail_closed() {
    let mut game = game();
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    assert_eq!(
        call(&mut game, "HasLOS", &[Value::None]),
        Value::Bool(false)
    );
    assert_eq!(
        call(
            &mut game,
            "PathToReference",
            &[Value::None, Value::Float(1.1)]
        ),
        Value::Bool(false)
    );
    assert_eq!(call(&mut game, "IsFlying", &[]), Value::Bool(false));
    let actor = game.world().actor_ref(id(ACTOR)).unwrap().clone();
    assert!(!actor.in_combat);
    assert!(!actor.ai.sleeping);
    assert!(actor.ai.path.is_empty());
}
