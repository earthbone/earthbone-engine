// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::LoadOrder;
use soe_game::{Game, EXTERIOR_CELL_LOADS_PER_UPDATE};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call_global(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s("Utility"), s(name), ident("result")];
    for index in 0..args.len() {
        let parameter = format!("arg{index}");
        function = function.param(&parameter, "Var");
        operands.push(ident(&parameter));
    }
    let bytes = ScriptBuilder::new("Scr51BudgetBreakerProbe", "Form")
        .function(
            function
                .ins(CallStatic, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game
        .vm
        .call_global("Scr51BudgetBreakerProbe", "Run", args)
        .unwrap();
    game.run_until_idle(8);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_51_budget_queries_reject_extra_arguments_for_both_global_signatures() {
    let mut game = Game::new(LoadOrder::new());

    assert_eq!(
        call_global(
            &mut game,
            "GetCurrentBudget",
            &[Value::Int(0), Value::Int(1)],
        ),
        Value::Int(0)
    );
    assert!(game
        .vm
        .log
        .contains("Utility.GetCurrentBudget expects 1 argument(s), got 2"));
    assert_eq!(
        call_global(&mut game, "OverBudget", &[Value::Int(0), Value::Int(1)],),
        Value::Bool(false)
    );
    assert!(game
        .vm
        .log
        .contains("Utility.OverBudget expects 1 argument(s), got 2"));
}

#[test]
fn scr_51_exterior_budget_reports_latest_count_and_strict_limit_boundary() {
    let mut game = Game::new(LoadOrder::new());

    game.record_exterior_cell_loads(EXTERIOR_CELL_LOADS_PER_UPDATE);
    assert_eq!(
        call_global(&mut game, "GetCurrentBudget", &[Value::Int(1)]),
        Value::Int(EXTERIOR_CELL_LOADS_PER_UPDATE as i32)
    );
    assert_eq!(
        call_global(&mut game, "OverBudget", &[Value::Int(1)]),
        Value::Bool(false)
    );

    game.record_exterior_cell_loads(0);
    assert_eq!(
        call_global(&mut game, "GetCurrentBudget", &[Value::Int(1)]),
        Value::Int(0)
    );
    assert_eq!(
        call_global(&mut game, "OverBudget", &[Value::Int(1)]),
        Value::Bool(false)
    );
}

#[test]
fn scr_51_over_budget_uses_the_runtime_vm_slice_limit() {
    let mut game = Game::new(LoadOrder::new());
    game.budget = 1_000;

    call_global(&mut game, "GetCurrentBudget", &[Value::Int(0)]);
    let usage = call_global(&mut game, "GetCurrentBudget", &[Value::Int(0)]);
    assert!(matches!(usage, Value::Int(value) if value > 1));

    game.budget = 1;
    assert_eq!(
        call_global(&mut game, "OverBudget", &[Value::Int(0)]),
        Value::Bool(true)
    );
}
