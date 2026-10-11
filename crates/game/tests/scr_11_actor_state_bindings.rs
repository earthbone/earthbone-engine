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

#[test]
fn scr_11_vehicle_motion_and_look_at_update_shared_actor_placement() {
    let mut game = fixture();
    let actor = id(0x801);
    let target = id(0x802);
    game.world_mut().reference(actor).unwrap().position = [10.0, 0.0, 0.0];
    game.world_mut().reference(target).unwrap().position = [0.0, 0.0, 0.0];

    let target_object = Value::Object(game.object_of_form(target));
    call(&mut game, actor, "SetVehicle", &[target_object]);
    game.world_mut().reference(target).unwrap().position = [5.0, 0.0, 0.0];
    game.world_mut().reference(target).unwrap().rotation[2] = std::f32::consts::FRAC_PI_2;
    game.world_mut().tick_actor_script_relationships();
    let actor_position = game.world_mut().reference(actor).unwrap().position;
    assert!(actor_position
        .iter()
        .zip([5.0, 10.0, 0.0])
        .all(|(actual, expected)| (actual - expected).abs() < 0.0001));
    assert!(
        (game.world_mut().reference(actor).unwrap().rotation[2] - std::f32::consts::FRAC_PI_2)
            .abs()
            < 0.0001
    );

    call(&mut game, actor, "SetVehicle", &[Value::None]);
    game.world_mut().reference(target).unwrap().position = [5.0, 10.0, 0.0];
    let target_object = Value::Object(game.object_of_form(target));
    call(
        &mut game,
        actor,
        "SetLookAt",
        &[target_object, Value::Bool(true)],
    );
    game.world_mut().reference(target).unwrap().position = [15.0, 10.0, 0.0];
    game.world_mut().tick_actor_script_relationships();
    let actor_yaw = game.world_mut().reference(actor).unwrap().rotation[2];
    assert!(actor_yaw.abs() < 0.0001);
    let before_pathing = game.world_mut().reference(actor).unwrap().position;
    game.world_mut()
        .ai_tick(0.2, &mut |_, _| soe_papyrus::value::ObjectId(0));
    let after_pathing = game.world_mut().reference(actor).unwrap().position;
    assert!(after_pathing[0] > before_pathing[0]);

    let target_object = Value::Object(game.object_of_form(target));
    call(
        &mut game,
        actor,
        "SetLookAt",
        &[target_object, Value::Bool(false)],
    );
    game.world_mut().tick_actor_script_relationships();
    let before_unpathing = game.world_mut().reference(actor).unwrap().position;
    game.world_mut()
        .ai_tick(0.2, &mut |_, _| soe_papyrus::value::ObjectId(0));
    assert_eq!(
        game.world_mut().reference(actor).unwrap().position,
        before_unpathing
    );
}

#[test]
fn scr_11_loaded_vehicle_relation_starts_from_restored_placements() {
    let game = fixture();
    let actor = id(0x801);
    let vehicle = id(0x802);
    game.world_mut().reference(actor).unwrap().position = [10.0, 0.0, 0.0];
    game.world_mut().reference(vehicle).unwrap().position = [0.0, 0.0, 0.0];
    game.world_mut()
        .set_actor_vehicle(actor, Some(vehicle))
        .unwrap();
    game.world_mut().tick_actor_script_relationships();
    game.world_mut().reference(vehicle).unwrap().position = [5.0, 0.0, 0.0];
    game.world_mut().tick_actor_script_relationships();
    let saved = game.world_mut().save_state();
    game.world_mut().reference(vehicle).unwrap().position = [100.0, 0.0, 0.0];
    game.world_mut().tick_actor_script_relationships();

    game.world_mut().load_state(&saved).unwrap();
    let restored = game.world_mut().reference(actor).unwrap().position;
    game.world_mut().tick_actor_script_relationships();
    assert_eq!(
        game.world_mut().reference(actor).unwrap().position,
        restored
    );
}
