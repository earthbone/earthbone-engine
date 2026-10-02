// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-driven edge checks for the SCR-11 Actor bindings.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{ai::Procedure, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::CallMethod, Value};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x801).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x802).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Scr11Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, actor: GlobalFormId, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr11Breaker{name}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("actor", "Form")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("actor"), ident("result")];
    for (index, _) in args.iter().enumerate() {
        let arg = format!("arg{index}");
        function = function.param(&arg, "Var");
        operands.push(ident(&arg));
    }
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut values = vec![Value::Object(game.object_of_form(actor))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn sit_state_requires_the_sit_procedure_and_reports_arrival() {
    let mut game = fixture();
    let actor = id(0x801);
    game.world_mut().actor(actor).unwrap().ai.arrived = true;
    assert_eq!(call(&mut game, actor, "GetSitState", &[]), Value::Int(0));
    game.world_mut().actor(actor).unwrap().ai.procedure = Procedure::Sit;
    assert_eq!(call(&mut game, actor, "GetSitState", &[]), Value::Int(3));
    game.world_mut().actor(actor).unwrap().ai.arrived = false;
    assert_eq!(call(&mut game, actor, "GetSitState", &[]), Value::Int(2));
}

#[test]
fn flying_state_preserves_each_defined_native_state() {
    let mut game = fixture();
    let actor = id(0x801);
    for state in 0_u8..=4 {
        game.world_mut().actor(actor).unwrap().flying_state = Some(state);
        assert_eq!(call(&mut game, actor, "GetFlyingState", &[]), Value::Int(i32::from(state)));
    }
    game.world_mut().actor(actor).unwrap().flying_state = None;
    game.world_mut().actor(actor).unwrap().ai.procedure = Procedure::Hover;
    assert_eq!(call(&mut game, actor, "GetFlyingState", &[]), Value::Int(3));
}

#[test]
fn set_essential_can_be_repeated_and_updates_inheriting_actor_state() {
    let mut game = fixture();
    let actor = id(0x801);
    call(&mut game, actor, "SetEssential", &[Value::Bool(true)]);
    assert!(game.world_mut().actor(actor).unwrap().essential);
    assert!(game.world_mut().actor_base(id(0x800)).unwrap().essential);
    call(&mut game, actor, "SetEssential", &[Value::Bool(false)]);
    assert!(!game.world_mut().actor(actor).unwrap().essential);
    assert!(!game.world_mut().actor_base(id(0x800)).unwrap().essential);
    call(&mut game, actor, "SetEssential", &[Value::Bool(false)]);
    assert!(!game.world_mut().actor(actor).unwrap().essential);
}

#[test]
fn invalid_vehicle_reference_does_not_detach_or_replace_current_vehicle() {
    let mut game = fixture();
    let actor = id(0x801);
    let vehicle = Value::Object(game.object_of_form(id(0x802)));
    call(&mut game, actor, "SetVehicle", &[vehicle]);
    let absent = Value::Object(game.object_of_form(id(0x999)));
    call(&mut game, actor, "SetVehicle", &[absent]);
    assert_eq!(game.world_mut().actor_vehicle(actor), Some(id(0x802)));
    assert!(game.traces().iter().any(|trace| trace.contains("not a placed reference")));
}

#[test]
fn invalid_look_target_keeps_target_and_pathing_until_explicit_clear() {
    let mut game = fixture();
    let actor = id(0x801);
    let target = Value::Object(game.object_of_form(id(0x802)));
    call(&mut game, actor, "SetLookAt", &[target, Value::Bool(true)]);
    let absent = Value::Object(game.object_of_form(id(0x999)));
    call(&mut game, actor, "SetLookAt", &[absent, Value::Bool(false)]);
    assert_eq!(game.world_mut().actor_look_at(actor), Some((id(0x802), true)));
    call(&mut game, actor, "ClearLookAt", &[]);
    assert_eq!(game.world_mut().actor_look_at(actor), None);
}

#[test]
fn vehicle_and_look_relationships_survive_world_save_load() {
    let mut game = fixture();
    let actor = id(0x801);
    let vehicle = Value::Object(game.object_of_form(id(0x802)));
    call(&mut game, actor, "SetVehicle", &[vehicle]);
    let target = Value::Object(game.object_of_form(id(0x802)));
    call(&mut game, actor, "SetLookAt", &[target, Value::Bool(true)]);
    let save = game.world_mut().save_state();
    let loaded = fixture();
    loaded.world_mut().load_state(&save).unwrap();
    assert_eq!(loaded.world_mut().actor_vehicle(actor), Some(id(0x802)));
    assert_eq!(loaded.world_mut().actor_look_at(actor), Some((id(0x802), true)));
}
