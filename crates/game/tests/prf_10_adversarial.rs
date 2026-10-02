// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn integer(game: &mut Game, name: &str, index: i32) -> Value {
    let function = FunctionBuilder::new("Read", "Int")
        .global()
        .param("index", "Int")
        .local("result", "Int")
        .ins(
            CallStatic,
            vec![s("Utility"), s(name), ident("result"), ident("index")],
        )
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new("Prf10Adversarial", "")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global("Prf10Adversarial", "Read", &[Value::Int(index)])
        .unwrap();
    assert!(game.run_until_idle(8));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn prf_10_invalid_budget_indices_use_fallback_for_signed_boundaries() {
    let mut game = Game::new(LoadOrder::new());
    for index in [i32::MIN, -1, 2, i32::MAX] {
        assert_eq!(integer(&mut game, "GetCurrentBudget", index), Value::Int(0));
    }
}
