// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn call(game: &mut Game, method: &str, args: &[Value], types: &[&str], returns: &str) -> Value {
    let script = format!("Scr17Probe{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let mut function = FunctionBuilder::new("Run", returns)
        .global()
        .param("actor", "Actor")
        .local("result", "Var");
    let mut operands = vec![s(method), ident("actor"), ident("result")];
    for (n, (ty, _)) in types.iter().zip(args).enumerate() {
        let name = format!("arg{n}");
        function = function.param(&name, ty);
        operands.push(ident(&name));
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
    let mut values = vec![Value::Object(game.object_of_form(id(ACTOR)))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE));
    plugin.push_top(Record::new(T(b"ACHR"), ACTOR).sub(T(b"NAME"), BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Scr17.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn scr_17_missing_intimidation_confidence_fails_closed() {
    let mut game = game();
    let result = call(&mut game, "WillIntimidateSucceed", &[], &[], "Bool");
    assert_eq!(result, Value::Bool(false));
    assert!(
        game.vm
            .log
            .messages()
            .iter()
            .all(|m| !m.to_string().contains("WillIntimidateSucceed"))
    );
}
