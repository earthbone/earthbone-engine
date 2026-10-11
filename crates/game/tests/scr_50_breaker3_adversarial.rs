// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script_name = format!("Scr50Breaker3{}", game.vm.time().to_bits());
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
fn scr_50_budget_name_requires_memory_initialization_even_for_negative_index() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(call(&mut game, "GetBudgetName", &[Value::Int(-1)]), Value::string(""));
    assert!(game
        .vm
        .log
        .contains("Utility.GetBudgetName: call GetCurrentMemory first"));
}

#[test]
fn scr_50_bad_current_memory_arity_does_not_initialize_tracker() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(call(&mut game, "GetCurrentMemory", &[Value::Int(1)]), Value::None);
    assert_eq!(call(&mut game, "GetBudgetCount", &[]), Value::None);
    assert!(game
        .vm
        .log
        .contains("Utility.GetBudgetCount: call GetCurrentMemory first"));
}

#[test]
fn scr_50_frame_rate_extrema_only_include_frames_during_capture() {
    let mut game = Game::new(LoadOrder::new());
    game.tick(1.0 / 10.0);
    call(&mut game, "StartFrameRateCapture", &[]);
    game.tick(1.0 / 30.0);
    game.tick(1.0 / 60.0);
    call(&mut game, "EndFrameRateCapture", &[]);
    game.tick(1.0 / 5.0);
    assert_eq!(call(&mut game, "GetMinFrameRate", &[]), Value::Float(30.0));
    assert_eq!(call(&mut game, "GetMaxFrameRate", &[]), Value::Float(60.0));
}
