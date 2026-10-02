// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn budget(game: &mut Game, index: i32) -> Value {
    let function = FunctionBuilder::new("Read", "Int")
        .global()
        .param("index", "Int")
        .local("result", "Int")
        .ins(
            CallStatic,
            vec![
                s("Utility"),
                s("GetCurrentBudget"),
                ident("result"),
                ident("index"),
            ],
        )
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new("Prf10Breaker2", "")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global("Prf10Breaker2", "Read", &[Value::Int(index)])
        .unwrap();
    assert!(game.run_until_idle(8));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn prf_10_measured_host_budget_counters_are_transient_across_save_load() {
    let mut game = Game::new(LoadOrder::new());
    game.record_exterior_cell_loads(2);
    let bytes = game.save_state().unwrap();

    let mut loaded = Game::new(LoadOrder::new());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(budget(&mut loaded, 1), Value::Int(0));
    assert!(matches!(budget(&mut loaded, 0), Value::Int(used) if used > 0));
}
