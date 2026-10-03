// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::LoadOrder;
use soe_game::{Game, EXTERIOR_CELL_LOADS_PER_UPDATE};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn query(game: &mut Game, name: &str, index: i32) -> Value {
    let script = ScriptBuilder::new("Scr51OverflowBudgetProbe", "Form")
        .function(
            FunctionBuilder::new("Run", "Var")
                .global()
                .param("index", "Int")
                .local("result", "Var")
                .ins(
                    CallStatic,
                    vec![s("Utility"), s(name), ident("result"), ident("index")],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let thread = game
        .vm
        .call_global("Scr51OverflowBudgetProbe", "Run", &[Value::Int(index)])
        .unwrap();
    game.run_until_idle(8);
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn exterior_budget_saturates_the_papyrus_integer_and_remains_over_limit() {
    let mut game = Game::new(LoadOrder::new());
    game.record_exterior_cell_loads(usize::MAX);

    assert_eq!(
        query(&mut game, "GetCurrentBudget", 1),
        Value::Int(i32::MAX)
    );
    assert_eq!(query(&mut game, "OverBudget", 1), Value::Bool(true));
    assert!(EXTERIOR_CELL_LOADS_PER_UPDATE < usize::MAX);
}
