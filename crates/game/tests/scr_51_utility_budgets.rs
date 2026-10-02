// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::LoadOrder;
use soe_game::{Game, EXTERIOR_CELL_LOADS_PER_UPDATE, PAPYRUS_INSTRUCTIONS_PER_SLICE};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call_global(game: &mut Game, name: &str, index: i32) -> Value {
    let function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("index", "Int")
        .local("result", "Var")
        .ins(
            CallStatic,
            vec![s("Utility"), s(name), ident("result"), ident("index")],
        )
        .ins(Return, vec![ident("result")])
        .build();
    let bytes = ScriptBuilder::new("Scr51UtilityBudgetProbe", "Form")
        .function(function)
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game
        .vm
        .call_global("Scr51UtilityBudgetProbe", "Run", &[Value::Int(index)])
        .unwrap();
    game.run_until_idle(8);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_51_get_current_budget_returns_each_engine_limit_and_invalid_indices_return_zero() {
    let mut game = Game::new(LoadOrder::new());

    assert_eq!(call_global(&mut game, "GetCurrentBudget", 0), Value::Int(0));
    assert_eq!(call_global(&mut game, "GetCurrentBudget", 1), Value::Int(0));
    assert!(matches!(
        call_global(&mut game, "GetCurrentBudget", 0),
        Value::Int(used) if used > 0 && used < PAPYRUS_INSTRUCTIONS_PER_SLICE as i32
    ));
    game.record_exterior_cell_loads(EXTERIOR_CELL_LOADS_PER_UPDATE);
    assert_eq!(
        call_global(&mut game, "GetCurrentBudget", 1),
        Value::Int(EXTERIOR_CELL_LOADS_PER_UPDATE as i32)
    );
    for index in [-1, 2, i32::MAX] {
        assert_eq!(
            call_global(&mut game, "GetCurrentBudget", index),
            Value::Int(0)
        );
    }
}

#[test]
fn scr_51_over_budget_reports_false_for_caps_and_invalid_indices() {
    let mut game = Game::new(LoadOrder::new());

    for index in [0, 1, -1, 2, i32::MAX] {
        assert_eq!(
            call_global(&mut game, "OverBudget", index),
            Value::Bool(false)
        );
    }
}

#[test]
fn scr_51_budget_queries_are_global_read_only_and_do_not_log_call_errors() {
    let mut game = Game::new(LoadOrder::new());
    let before = game.world().state_hash();

    assert_eq!(call_global(&mut game, "GetCurrentBudget", 0), Value::Int(0));
    assert_eq!(call_global(&mut game, "OverBudget", 1), Value::Bool(false));

    assert_eq!(game.world().state_hash(), before);
    assert!(game.traces().is_empty());
}
