// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Source-driven edge checks for SCR-11 Actor bindings.

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
    for object in [0x801, 0x802, 0x803] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    }
    let mut order = LoadOrder::new();
    order.add("Scr11Breaker3.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, actor: GlobalFormId, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr11Breaker3{name}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("actor", "Form")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("actor"), ident("result")];
    for index in 0..args.len() {
        let argument = format!("arg{index}");
        function = function.param(&argument, "Var");
        operands.push(ident(&argument));
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
fn set_essential_clears_protection_for_every_actor_sharing_the_base() {
    let mut game = fixture();
    let base = id(0x800);
    let first = id(0x801);
    let sibling = id(0x803);
    game.world_mut().set_actor_base_protected(base, true);
    assert!(game.world_mut().actor(first).unwrap().protected);
    assert!(game.world_mut().actor(sibling).unwrap().protected);

    call(&mut game, first, "SetEssential", &[Value::Bool(true)]);

    assert!(game.world_mut().actor_base(base).unwrap().essential);
    for actor in [first, sibling] {
        let (essential, protected) = {
            let mut world = game.world_mut();
            let state = world.actor(actor).unwrap();
            (state.essential, state.protected)
        };
        assert!(essential);
        assert!(!protected);
    }
}

#[test]
fn vehicle_tracking_applies_each_motion_delta_once_and_stops_after_detach() {
    let mut game = fixture();
    let actor = id(0x801);
    let vehicle = id(0x802);
    game.world_mut().reference(actor).unwrap().position = [10.0, 3.0, 2.0];
    game.world_mut().reference(vehicle).unwrap().position = [0.0, 0.0, 0.0];
    let vehicle_value = Value::Object(game.object_of_form(vehicle));
    call(
        &mut game,
        actor,
        "SetVehicle",
        &[vehicle_value],
    );

    game.world_mut().reference(vehicle).unwrap().position = [4.0, -2.0, 5.0];
    game.world_mut().tick_actor_script_relationships();
    let expected = [14.0, 1.0, 7.0];
    for _ in 0..3 {
        game.world_mut().tick_actor_script_relationships();
        let actual = game.world_mut().reference(actor).unwrap().position;
        assert!(actual.iter().zip(expected).all(|(a, b)| (a - b).abs() < 0.0001));
    }

    call(&mut game, actor, "SetVehicle", &[Value::None]);
    game.world_mut().reference(vehicle).unwrap().position = [9.0, 9.0, 9.0];
    game.world_mut().tick_actor_script_relationships();
    assert_eq!(game.world_mut().reference(actor).unwrap().position, expected);
}
