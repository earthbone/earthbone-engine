// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::LoadOrder;
use soe_game::{Game, EXTERIOR_CELL_LOADS_PER_UPDATE, PAPYRUS_INSTRUCTIONS_PER_SLICE};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call(game: &mut Game, name: &str, index: Option<i32>) -> Value {
    let mut function = FunctionBuilder::new("Read", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s("Utility"), s(name), ident("result")];
    let args = if let Some(index) = index {
        function = function.param("index", "Int");
        operands.push(ident("index"));
        vec![Value::Int(index)]
    } else {
        Vec::new()
    };
    game.load_script(
        &ScriptBuilder::new("Prf10BreakerProbe", "Form")
            .function(
                function
                    .ins(CallStatic, operands)
                    .ins(Return, vec![ident("result")])
                    .build(),
            )
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global("Prf10BreakerProbe", "Read", &args)
        .unwrap();
    assert!(game.run_until_idle(8));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn budget_categories_match_the_runtime_limits() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(call(&mut game, "GetBudgetCount", None), Value::Int(2));
    assert_eq!(
        call(&mut game, "GetBudgetName", Some(0)),
        Value::string("Papyrus instructions per slice")
    );
    assert_eq!(
        call(&mut game, "GetBudgetName", Some(1)),
        Value::string("Exterior cell loads per update")
    );
    assert_eq!(PAPYRUS_INSTRUCTIONS_PER_SLICE, 100_000);
    assert_eq!(EXTERIOR_CELL_LOADS_PER_UPDATE, 2);
}

#[test]
fn invalid_budget_indices_return_type_specific_fallbacks() {
    let mut game = Game::new(LoadOrder::new());
    for index in [i32::MIN, -1, 2, i32::MAX] {
        assert_eq!(call(&mut game, "GetBudgetName", Some(index)), Value::None);
        assert_eq!(
            call(&mut game, "GetCurrentBudget", Some(index)),
            Value::Int(0)
        );
        assert_eq!(
            call(&mut game, "OverBudget", Some(index)),
            Value::Bool(false)
        );
    }
}

#[test]
fn streaming_counter_uses_measured_loads_and_strict_limit_boundary() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(call(&mut game, "GetCurrentBudget", Some(1)), Value::Int(0));
    assert_eq!(call(&mut game, "OverBudget", Some(1)), Value::Bool(false));

    game.record_exterior_cell_loads(EXTERIOR_CELL_LOADS_PER_UPDATE);
    assert_eq!(
        call(&mut game, "GetCurrentBudget", Some(1)),
        Value::Int(EXTERIOR_CELL_LOADS_PER_UPDATE as i32)
    );
    assert_eq!(call(&mut game, "OverBudget", Some(1)), Value::Bool(false));

    game.record_exterior_cell_loads(EXTERIOR_CELL_LOADS_PER_UPDATE + 1);
    assert_eq!(
        call(&mut game, "GetCurrentBudget", Some(1)),
        Value::Int((EXTERIOR_CELL_LOADS_PER_UPDATE + 1) as i32)
    );
    assert_eq!(call(&mut game, "OverBudget", Some(1)), Value::Bool(true));

    game.record_exterior_cell_loads(0);
    assert_eq!(call(&mut game, "GetCurrentBudget", Some(1)), Value::Int(0));
    assert_eq!(call(&mut game, "OverBudget", Some(1)), Value::Bool(false));
}

#[test]
fn vm_counter_reports_scheduler_work_instead_of_the_configured_limit() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(call(&mut game, "GetBudgetCount", None), Value::Int(2));
    let measured = call(&mut game, "GetCurrentBudget", Some(0));
    assert!(
        matches!(measured, Value::Int(n) if n > 0 && n < PAPYRUS_INSTRUCTIONS_PER_SLICE as i32)
    );
    assert_eq!(call(&mut game, "OverBudget", Some(0)), Value::Bool(false));
}
