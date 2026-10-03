// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script_name = format!("Scr50{}", game.vm.time().to_bits());
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
fn scr_50_beta_frame_rate_queries_report_missing_capture_provider() {
    let mut game = Game::new(LoadOrder::new());
    for name in ["GetMinFrameRate", "GetMaxFrameRate"] {
        assert_eq!(call(&mut game, name, &[]), Value::None);
        assert!(game.vm.log.contains(&format!(
            "Utility.{name}: beta frame-rate capture is unavailable"
        )));
    }
}

#[test]
fn scr_50_frame_rate_queries_read_the_active_capture() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(call(&mut game, "GetCurrentMemory", &[]), Value::None);
    assert!(game
        .vm
        .log
        .contains("Utility.GetCurrentMemory: memory tracking is unavailable"));
    assert_eq!(call(&mut game, "GetBudgetCount", &[]), Value::Int(2));
    assert!(!game
        .vm
        .log
        .contains("Utility.GetBudgetCount: memory tracking is unavailable"));
    call(&mut game, "StartFrameRateCapture", &[]);
    game.tick(1.0 / 60.0);
    game.tick(1.0 / 30.0);
    assert_eq!(call(&mut game, "GetMinFrameRate", &[]), Value::Float(30.0));
    assert_eq!(call(&mut game, "GetMaxFrameRate", &[]), Value::Float(60.0));
}

#[test]
fn scr_50_current_memory_requires_tracking_and_validates_arity() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(call(&mut game, "GetCurrentMemory", &[]), Value::None);
    assert!(game
        .vm
        .log
        .contains("Utility.GetCurrentMemory: memory tracking is unavailable"));
    assert_eq!(
        call(&mut game, "GetCurrentMemory", &[Value::Int(1)]),
        Value::None
    );
    assert!(game
        .vm
        .log
        .contains("Utility.GetCurrentMemory expects 0 argument(s), got 1"));
}

#[test]
fn scr_50_budget_count_and_names_read_engine_categories() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(call(&mut game, "GetBudgetCount", &[]), Value::Int(2));
    assert_eq!(
        call(&mut game, "GetBudgetName", &[Value::Int(0)]),
        Value::string("Papyrus instructions per slice")
    );
    assert_eq!(
        call(&mut game, "GetBudgetName", &[Value::Int(1)]),
        Value::string("Exterior cell loads per update")
    );
    assert_eq!(
        call(&mut game, "GetBudgetName", &[Value::Int(-1)]),
        Value::string("")
    );
    assert_eq!(
        call(&mut game, "GetBudgetName", &[Value::Bool(true)]),
        Value::string("")
    );
    assert!(game
        .vm
        .log
        .contains("Utility.GetBudgetName expects an integer budget index"));
    assert_eq!(
        call(&mut game, "GetBudgetCount", &[Value::Int(0)]),
        Value::Int(0)
    );
    assert!(game
        .vm
        .log
        .contains("Utility.GetBudgetCount expects 0 argument(s), got 1"));
    assert_eq!(call(&mut game, "GetBudgetName", &[]), Value::string(""));
    assert!(game
        .vm
        .log
        .contains("Utility.GetBudgetName expects 1 argument(s), got 0"));
}

#[test]
fn scr_50_global_queries_reject_unexpected_arguments() {
    let mut game = Game::new(LoadOrder::new());
    for name in ["GetMinFrameRate", "GetMaxFrameRate"] {
        assert_eq!(call(&mut game, name, &[Value::Int(1)]), Value::None);
        assert!(game
            .vm
            .log
            .contains(&format!("Utility.{name} expects 0 argument(s), got 1")));
    }
    assert_eq!(
        call(&mut game, "GetBudgetCount", &[Value::Int(0)]),
        Value::Int(0)
    );
    assert!(game
        .vm
        .log
        .contains("Utility.GetBudgetCount expects 0 argument(s), got 1"));
}
