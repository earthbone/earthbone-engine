// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-driven checks for SCR-11 actor state bindings.

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
    let mut order = LoadOrder::new();
    order.add("Scr11Breaker5.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, target: GlobalFormId, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr11Breaker5{name}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Form")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("target"), ident("result")];
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
    let mut values = vec![Value::Object(game.object_of_form(target))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn sit_transition_does_not_override_wants_to_sit_before_arrival() {
    let mut game = fixture();
    let actor = id(0x801);
    let state = game.world_mut().actor(actor).unwrap();
    state.ai.procedure = Procedure::Sit;
    state.ai.arrived = false;
    game.world_mut()
        .set_actor_sit_wants_to_stand(actor, true)
        .unwrap();

    assert_eq!(call(&mut game, actor, "GetSitState", &[]), Value::Int(2));
}

#[test]
fn making_actor_base_essential_clears_protection_for_inheritors() {
    let mut game = fixture();
    let base = id(0x800);
    let actor = id(0x801);
    game.world_mut().set_actor_base_protected(base, true);
    assert!(game.world_mut().actor_base(base).unwrap().protected);
    assert!(game.world_mut().actor(actor).unwrap().protected);

    call(&mut game, base, "SetEssential", &[Value::Bool(true)]);

    assert!(game.world_mut().actor_base(base).unwrap().essential);
    assert!(game.world_mut().actor(actor).unwrap().essential);
    assert!(!game.world_mut().actor_base(base).unwrap().protected);
    assert!(!game.world_mut().actor(actor).unwrap().protected);
}
