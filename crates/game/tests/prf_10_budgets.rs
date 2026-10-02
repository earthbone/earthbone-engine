// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::LoadOrder;
use soe_game::{Game, PAPYRUS_INSTRUCTIONS_PER_SLICE};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

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
    assert!(game.run_until_idle(8));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn prf_10_budget_queries_expose_limits_and_diagnose_missing_memory_tracking() {
    let mut game = Game::new(LoadOrder::new());

    assert_eq!(call_global(&mut game, "GetCurrentMemory", &[]), Value::None);
    assert!(
        game.vm
            .log
            .contains("Utility.GetCurrentMemory: memory tracking is unavailable")
    );
    assert_eq!(call_global(&mut game, "GetBudgetCount", &[]), Value::Int(2));
    assert_eq!(
        call_global(&mut game, "GetBudgetName", &[Value::Int(0)]),
        Value::string("Papyrus instructions per slice")
    );
    assert!(matches!(
        call_global(&mut game, "GetCurrentBudget", &[Value::Int(0)]),
        Value::Int(n) if n > 0 && n < PAPYRUS_INSTRUCTIONS_PER_SLICE as i32
    ));
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
        Value::Int(0)
    );
    assert_eq!(
        call_global(&mut game, "OverBudget", &[Value::Int(1)]),
        Value::Bool(false)
    );

    for index in [-1, 2, i32::MAX] {
        assert_eq!(
            call_global(&mut game, "GetBudgetName", &[Value::Int(index)]),
            Value::None
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

#[test]
fn prf_10_current_budget_reads_measured_vm_and_streaming_usage() {
    let mut game = Game::new(LoadOrder::new());
    // This call executes Papyrus instructions. The scheduler records the count
    // when its slice returns, which the following query observes.
    assert_eq!(call_global(&mut game, "GetBudgetCount", &[]), Value::Int(2));
    let vm_usage = call_global(&mut game, "GetCurrentBudget", &[Value::Int(0)]);
    assert!(
        matches!(vm_usage, Value::Int(n) if n > 0 && n < PAPYRUS_INSTRUCTIONS_PER_SLICE as i32)
    );
    assert_eq!(
        call_global(&mut game, "GetCurrentBudget", &[Value::Int(0)]),
        vm_usage
    );
    assert_eq!(
        call_global(&mut game, "OverBudget", &[Value::Int(0)]),
        Value::Bool(false)
    );

    game.record_exterior_cell_loads(1);
    assert_eq!(
        call_global(&mut game, "GetCurrentBudget", &[Value::Int(1)]),
        Value::Int(1)
    );
    assert_eq!(
        call_global(&mut game, "OverBudget", &[Value::Int(1)]),
        Value::Bool(false)
    );
}
