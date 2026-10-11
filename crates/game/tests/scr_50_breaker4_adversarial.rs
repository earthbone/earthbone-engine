// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script_name = format!("Scr50Edge{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "Var").global().local("result", "Var");
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
fn scr_50_negative_budget_name_still_requires_memory_initialization() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(call(&mut game, "GetBudgetName", &[Value::Int(-1)]), Value::string(""));
    assert!(game
        .vm
        .log
        .contains("Utility.GetBudgetName: call GetCurrentMemory first"));
}

#[test]
fn scr_50_bad_current_memory_arity_does_not_initialize_tracking() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(
        call(&mut game, "GetCurrentMemory", &[Value::Int(1)]),
        Value::None
    );
    assert_eq!(call(&mut game, "GetBudgetCount", &[]), Value::None);
    assert!(game
        .vm
        .log
        .contains("Utility.GetBudgetCount: call GetCurrentMemory first"));
}

#[test]
fn scr_50_memory_query_counts_allocations_from_non_papyrus_systems() {
    let mut game = Game::new(LoadOrder::new());
    let _ = call(&mut game, "GetCurrentMemory", &[]);
    soe_trace::memory_allocated(soe_trace::System::Streaming, 8192);
    let memory = call(&mut game, "GetCurrentMemory", &[]);
    assert!(matches!(memory, Value::Int(value) if value >= 8192));
    soe_trace::memory_freed(soe_trace::System::Streaming, 8192);
}

#[test]
fn scr_50_frame_rate_queries_reject_arguments_without_sampling_a_frame() {
    let mut game = Game::new(LoadOrder::new());
    for name in ["GetMinFrameRate", "GetMaxFrameRate"] {
        assert_eq!(call(&mut game, name, &[Value::Int(1)]), Value::None);
        assert!(game
            .vm
            .log
            .contains(&format!("Utility.{name} expects 0 argument(s), got 1")));
        assert_eq!(call(&mut game, name, &[]), Value::None);
        assert!(game
            .vm
            .log
            .contains(&format!("Utility.{name}: beta frame-rate capture is unavailable")));
    }
}

#[test]
fn scr_50_restarting_capture_cannot_discard_frames_from_an_active_capture() {
    let mut game = Game::new(LoadOrder::new());
    call(&mut game, "StartFrameRateCapture", &[]);
    game.tick(1.0 / 20.0);
    call(&mut game, "StartFrameRateCapture", &[]);
    game.tick(1.0 / 40.0);
    call(&mut game, "EndFrameRateCapture", &[]);

    assert_eq!(call(&mut game, "GetMinFrameRate", &[]), Value::Float(20.0));
    assert_eq!(call(&mut game, "GetMaxFrameRate", &[]), Value::Float(40.0));
    assert!(game
        .vm
        .log
        .contains("Utility.StartFrameRateCapture called while capture is active"));
}
