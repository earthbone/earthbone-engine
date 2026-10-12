// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Source-driven runtime edge checks for the SCR-02 Actor contract audit.

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
    let mut order = LoadOrder::new();
    order.add("Scr02Breaker6.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, actor: GlobalFormId, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr02Breaker6{name}");
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
fn scr_02_actorbase_set_essential_rejects_missing_or_wrong_typed_argument() {
    // CK ActorBase.SetEssential requires one Bool argument.
    let mut game = fixture();
    let base = id(0x800);

    assert_eq!(call(&mut game, base, "SetEssential", &[]), Value::None);
    assert!(!game.world_mut().actor_base(base).unwrap().essential);
    assert_eq!(
        call(&mut game, base, "SetEssential", &[Value::Int(1)]),
        Value::None
    );
    assert!(!game.world_mut().actor_base(base).unwrap().essential);
    assert!(
        game.traces()
            .iter()
            .filter(|trace| trace.contains("ActorBase.SetEssential expects one Bool"))
            .count()
            >= 2
    );
}

#[test]
fn scr_02_actor_enable_ai_defaults_to_true_after_disable() {
    // CK EnableAI has an optional abEnable defaulting to true.
    let mut game = fixture();
    let actor = id(0x801);

    assert_eq!(
        call(&mut game, actor, "EnableAI", &[Value::Bool(false)]),
        Value::None
    );
    assert!(game.world_mut().actor(actor).unwrap().ai_disabled);
    assert_eq!(call(&mut game, actor, "EnableAI", &[]), Value::None);
    assert!(!game.world_mut().actor(actor).unwrap().ai_disabled);
}

#[test]
fn scr_02_actor_set_allow_flying_defaults_to_true() {
    // CK Actor.psc declares abAllowed = true for SetAllowFlying.
    let mut game = fixture();
    let actor = id(0x801);

    call(&mut game, actor, "SetAllowFlying", &[Value::Bool(false)]);
    assert!(!game.world_mut().actor(actor).unwrap().allow_flying);
    assert_eq!(call(&mut game, actor, "SetAllowFlying", &[]), Value::None);
    assert!(game.world_mut().actor(actor).unwrap().allow_flying);
}
