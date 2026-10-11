// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Source-driven edge checks for SCR-11 actor state bindings.

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
    order.add("Scr11Breaker4.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, actor: GlobalFormId, name: &str) -> Value {
    let script = format!("Scr11Breaker4{name}");
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            FunctionBuilder::new("Run", "Var")
                .global()
                .param("actor", "Form")
                .local("result", "Var")
                .ins(CallMethod, vec![s(name), ident("actor"), ident("result")])
                .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let target = Value::Object(game.object_of_form(actor));
    let thread = game.vm.call_global(&script, "Run", &[target]).unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_11_sit_transition_is_transient_across_world_load() {
    let mut game = fixture();
    let actor = id(0x801);
    game.world_mut().actor(actor).unwrap().ai.procedure = Procedure::Sit;
    game.world_mut().actor(actor).unwrap().ai.arrived = true;
    game.world_mut()
        .set_actor_sit_wants_to_stand(actor, true)
        .unwrap();
    assert_eq!(call(&mut game, actor, "GetSitState"), Value::Int(4));

    let save = game.world_mut().save_state();
    game.world_mut().load_state(&save).unwrap();
    assert_eq!(call(&mut game, actor, "GetSitState"), Value::Int(3));
}

#[test]
fn scr_11_flying_state_never_returns_undefined_ck_values() {
    let mut game = fixture();
    let actor = id(0x801);
    game.world_mut().actor(actor).unwrap().flying_state = Some(5);

    let value = call(&mut game, actor, "GetFlyingState");
    assert!(
        matches!(value, Value::Int(0..=4)),
        "unexpected state: {value:?}"
    );
}

#[test]
fn scr_11_set_essential_uses_its_ck_actorbase_type() {
    let game = Game::new(LoadOrder::new());

    assert!(game.vm.natives.has_method("ActorBase", "SetEssential"));
    assert!(!game.vm.natives.has_method("Actor", "SetEssential"));
}
