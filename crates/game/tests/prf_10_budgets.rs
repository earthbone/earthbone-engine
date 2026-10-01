// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::LoadOrder;
use soe_game::{Game, EXTERIOR_CELL_LOADS_PER_UPDATE, PAPYRUS_INSTRUCTIONS_PER_SLICE};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call_global(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s("Utility"), s(name), ident("result")];
    for (index, arg) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        function = function.param(&parameter, "Int");
        operands.push(ident(&parameter));
        assert!(matches!(arg, Value::Int(_)));
    }
    let bytes = ScriptBuilder::new("Prf10BudgetProbe", "Form")
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
        .call_global("Prf10BudgetProbe", "Run", args)
        .unwrap();
    game.run_until_idle(8);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn prf_10_budget_categories_expose_engine_limits_and_ck_fallbacks() {
    let mut game = Game::new(LoadOrder::new());

    assert_eq!(call_global(&mut game, "GetBudgetCount", &[]), Value::Int(2));
    assert_eq!(
        call_global(&mut game, "GetBudgetName", &[Value::Int(0)]),
        Value::string("Papyrus instructions per slice")
    );
    assert_eq!(
        call_global(&mut game, "GetCurrentBudget", &[Value::Int(0)]),
        Value::Int(PAPYRUS_INSTRUCTIONS_PER_SLICE as i32)
    );
    assert_eq!(
        call_global(&mut game, "OverBudget", &[Value::Int(0)]),
        Value::Bool(false)
    );
    assert_eq!(
        call_global(&mut game, "GetBudgetName", &[Value::Int(1)]),
        Value::string("Exterior cell loads per update")
    );
    assert_eq!(
        call_global(&mut game, "GetCurrentBudget", &[Value::Int(1)]),
        Value::Int(EXTERIOR_CELL_LOADS_PER_UPDATE as i32)
    );
    assert_eq!(
        call_global(&mut game, "OverBudget", &[Value::Int(1)]),
        Value::Bool(false)
    );

    for index in [-1, 2, i32::MAX] {
        assert_eq!(
            call_global(&mut game, "GetBudgetName", &[Value::Int(index)]),
            Value::string("")
        );
        assert_eq!(
            call_global(&mut game, "GetCurrentBudget", &[Value::Int(index)]),
            Value::Int(0)
        );
        assert_eq!(
            call_global(&mut game, "OverBudget", &[Value::Int(index)]),
            Value::Bool(false)
        );
    }
}
