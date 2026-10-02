// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::LoadOrder;
use soe_game::{Game, EXTERIOR_CELL_LOADS_PER_UPDATE, PAPYRUS_INSTRUCTIONS_PER_SLICE};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call_global(game: &mut Game, name: &str, index: i32) -> Value {
    call_global_args(game, name, &[Value::Int(index)])
}

fn call_global_args(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s("Utility"), s(name), ident("result")];
    for index in 0..args.len() {
        let parameter = format!("arg{index}");
        function = function.param(&parameter, "Var");
        operands.push(ident(&parameter));
    }
    function = function
        .ins(CallStatic, operands)
        .ins(Return, vec![ident("result")]);
    let bytes = ScriptBuilder::new("Scr51UtilityBudgetProbe", "Form")
        .function(function.build())
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game
        .vm
        .call_global("Scr51UtilityBudgetProbe", "Run", args)
        .unwrap();
    game.run_until_idle(8);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_51_get_current_budget_returns_measured_usage_and_invalid_indices_return_zero() {
    let mut game = Game::new(LoadOrder::new());

    call_global(&mut game, "OverBudget", 0);
    let instructions = call_global(&mut game, "GetCurrentBudget", 0);
    assert!(matches!(instructions, Value::Int(value) if value > 0));
    assert!(
        matches!(instructions, Value::Int(value) if value <= PAPYRUS_INSTRUCTIONS_PER_SLICE as i32)
    );
    assert_eq!(call_global(&mut game, "GetCurrentBudget", 1), Value::Int(0));
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

#[test]
fn scr_51_budget_queries_report_measured_vm_and_exterior_usage() {
    let mut game = Game::new(LoadOrder::new());

    call_global(&mut game, "OverBudget", 0);
    let instructions = call_global(&mut game, "GetCurrentBudget", 0);
    assert!(matches!(instructions, Value::Int(value) if value > 0));
    assert!(
        matches!(instructions, Value::Int(value) if value <= PAPYRUS_INSTRUCTIONS_PER_SLICE as i32)
    );
    assert_eq!(call_global(&mut game, "OverBudget", 0), Value::Bool(false));

    game.record_exterior_cell_loads(1);
    assert_eq!(call_global(&mut game, "GetCurrentBudget", 1), Value::Int(1));
    assert_eq!(call_global(&mut game, "OverBudget", 1), Value::Bool(false));

    game.record_exterior_cell_loads(EXTERIOR_CELL_LOADS_PER_UPDATE + 1);
    assert_eq!(call_global(&mut game, "OverBudget", 1), Value::Bool(true));
}

#[test]
fn scr_51_budget_queries_validate_arity_and_integer_indices() {
    let mut game = Game::new(LoadOrder::new());

    assert_eq!(
        call_global_args(&mut game, "GetCurrentBudget", &[]),
        Value::Int(0)
    );
    assert!(game
        .vm
        .log
        .contains("Utility.GetCurrentBudget expects 1 argument(s), got 0"));
    assert_eq!(
        call_global_args(&mut game, "GetCurrentBudget", &[Value::Bool(true)]),
        Value::Int(0)
    );
    assert!(game
        .vm
        .log
        .contains("Utility.GetCurrentBudget expects an integer budget index"));

    assert_eq!(
        call_global_args(&mut game, "OverBudget", &[]),
        Value::Bool(false)
    );
    assert!(game
        .vm
        .log
        .contains("Utility.OverBudget expects 1 argument(s), got 0"));
    assert_eq!(
        call_global_args(&mut game, "OverBudget", &[Value::Bool(true)]),
        Value::Bool(false)
    );
    assert!(game
        .vm
        .log
        .contains("Utility.OverBudget expects an integer budget index"));
}
