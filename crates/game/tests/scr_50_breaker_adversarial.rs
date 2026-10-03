// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script_name = format!("Scr50Breaker{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s("Utility"), s(name), ident("result")];
    for (index, _) in args.iter().enumerate() {
        let argument = format!("arg{index}");
        function = function.param(&argument, "Var");
        operands.push(ident(&argument));
    }
    let bytes = ScriptBuilder::new(&script_name, "")
        .function(
            function
                .ins(CallStatic, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game.vm.call_global(&script_name, "Run", args).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_50_memory_budget_count_is_unavailable_without_memory_tracking() {
    let mut game = Game::new(LoadOrder::new());

    assert_eq!(call(&mut game, "GetCurrentMemory", &[]), Value::None);
    assert_eq!(call(&mut game, "GetBudgetCount", &[]), Value::None);
    assert!(game
        .vm
        .log
        .contains("Utility.GetBudgetCount: memory tracking is unavailable"));
}

#[test]
fn scr_50_budget_names_do_not_report_scheduler_limits_as_memory_budgets() {
    let mut game = Game::new(LoadOrder::new());

    for index in [0, 1] {
        let name = call(&mut game, "GetBudgetName", &[Value::Int(index)]);
        assert_ne!(name, Value::string("Papyrus instructions per slice"));
        assert_ne!(name, Value::string("Exterior cell loads per update"));
    }
}

#[test]
fn scr_50_restarting_capture_discards_the_previous_extrema() {
    let mut game = Game::new(LoadOrder::new());

    call(&mut game, "StartFrameRateCapture", &[]);
    game.tick(1.0 / 20.0);
    call(&mut game, "EndFrameRateCapture", &[]);
    assert_eq!(call(&mut game, "GetMinFrameRate", &[]), Value::Float(20.0));
    assert_eq!(call(&mut game, "GetMaxFrameRate", &[]), Value::Float(20.0));

    call(&mut game, "StartFrameRateCapture", &[]);
    game.tick(1.0 / 60.0);
    assert_eq!(call(&mut game, "GetMinFrameRate", &[]), Value::Float(60.0));
    assert_eq!(call(&mut game, "GetMaxFrameRate", &[]), Value::Float(60.0));
}
