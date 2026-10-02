// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-11 actor state methods use the shared actor and reference records.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
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
    order.add("Scr11.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, actor: GlobalFormId, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr11{name}");
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
    let target = Value::Object(game.object_of_form(actor));
    let mut values = vec![target];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_11_sit_flight_and_essential_methods_read_and_write_actor_state() {
    let mut game = fixture();
    let actor = id(0x801);
    assert_eq!(call(&mut game, actor, "GetSitState", &[]), Value::Int(0));
    game.world_mut().actor(actor).unwrap().ai.procedure = soe_game::ai::Procedure::Sit;
    assert_eq!(call(&mut game, actor, "GetSitState", &[]), Value::Int(2));
    game.world_mut().actor(actor).unwrap().ai.arrived = true;
    assert_eq!(call(&mut game, actor, "GetSitState", &[]), Value::Int(3));
    game.world_mut().actor(actor).unwrap().flying_state = Some(4);
    assert_eq!(call(&mut game, actor, "GetFlyingState", &[]), Value::Int(4));
    call(&mut game, actor, "SetEssential", &[Value::Bool(true)]);
    assert!(game.world_mut().actor(actor).unwrap().essential);
    assert!(game.world_mut().actor_base(id(0x800)).unwrap().essential);
}

#[test]
fn scr_11_vehicle_method_validates_and_persists_attachment_selection() {
    let mut game = fixture();
    let actor = id(0x801);
    let vehicle = Value::Object(game.object_of_form(id(0x802)));
    call(&mut game, actor, "SetVehicle", &[vehicle]);
    assert_eq!(game.world_mut().actor_vehicle(actor), Some(id(0x802)));
    call(&mut game, actor, "SetVehicle", &[Value::None]);
    assert_eq!(game.world_mut().actor_vehicle(actor), None);
    let self_ref = Value::Object(game.object_of_form(actor));
    call(&mut game, actor, "SetVehicle", &[self_ref]);
    assert_eq!(game.world_mut().actor_vehicle(actor), None);

    call(&mut game, actor, "SetVehicle", &[Value::Int(3)]);
    assert_eq!(game.world_mut().actor_vehicle(actor), None);
    assert!(game
        .traces()
        .iter()
        .any(|trace| trace.contains("SetVehicle expects")));

    let vehicle = Value::Object(game.object_of_form(id(0x802)));
    call(&mut game, actor, "SetVehicle", &[vehicle]);
    let save = game.world_mut().save_state();
    let loaded = fixture();
    loaded.world_mut().load_state(&save).unwrap();
    assert_eq!(loaded.world_mut().actor_vehicle(actor), Some(id(0x802)));
}

#[test]
fn scr_11_look_at_method_keeps_target_pathing_and_clear_effect() {
    let mut game = fixture();
    let actor = id(0x801);
    let target = Value::Object(game.object_of_form(id(0x802)));
    call(&mut game, actor, "SetLookAt", &[target, Value::Bool(true)]);
    assert_eq!(
        game.world_mut().actor_look_at(actor),
        Some((id(0x802), true))
    );

    call(
        &mut game,
        actor,
        "SetLookAt",
        &[Value::Int(3), Value::Bool(false)],
    );
    assert_eq!(
        game.world_mut().actor_look_at(actor),
        Some((id(0x802), true))
    );
    assert!(game
        .traces()
        .iter()
        .any(|trace| trace.contains("SetLookAt expects")));

    let save = game.world_mut().save_state();
    let loaded = fixture();
    loaded.world_mut().load_state(&save).unwrap();
    assert_eq!(
        loaded.world_mut().actor_look_at(actor),
        Some((id(0x802), true))
    );
    call(&mut game, actor, "ClearLookAt", &[]);
    assert_eq!(game.world_mut().actor_look_at(actor), None);
    let target = Value::Object(game.object_of_form(id(0x802)));
    call(&mut game, actor, "SetLookAt", &[target, Value::Bool(true)]);
    let saved = game.world_mut().save_state();
    loaded.world_mut().load_state(&saved).unwrap();
    assert_eq!(
        loaded.world_mut().actor_look_at(actor),
        Some((id(0x802), true))
    );
}
