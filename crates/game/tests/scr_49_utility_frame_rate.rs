// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

static PROBE_ID: AtomicUsize = AtomicUsize::new(0);

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::Bool(_) => "Bool",
        Value::Str(_) => "String",
        Value::Object(_) => "Form",
        Value::None | Value::Array(_) | Value::Struct(_) => "Var",
    }
}

fn call_global(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr49Probe{}", PROBE_ID.fetch_add(1, Ordering::Relaxed));
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s("Utility"), s(name), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        function = function.param(&parameter, type_of(value));
        operands.push(ident(&parameter));
    }
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            function
                .ins(CallStatic, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game.vm.call_global(&script, "Run", args).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn has_log(game: &Game, expected: &str) -> bool {
    game.traces().iter().any(|entry| entry.contains(expected))
}

#[test]
fn scr_49_menu_query_tracks_host_pause_state() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(
        call_global(&mut game, "IsInMenuMode", &[]),
        Value::Bool(false)
    );
    game.tick_menu_mode(1.0 / 60.0, true);
    assert_eq!(
        call_global(&mut game, "IsInMenuMode", &[]),
        Value::Bool(true)
    );
    game.tick_menu_mode(1.0 / 60.0, false);
    assert_eq!(
        call_global(&mut game, "IsInMenuMode", &[]),
        Value::Bool(false)
    );
}

#[test]
fn scr_49_frame_rate_capture_validates_count_and_formats_recent_samples() {
    let mut game = Game::new(LoadOrder::new());
    game.tick(1.0 / 60.0);
    game.tick(1.0 / 30.0);
    assert_eq!(
        call_global(&mut game, "CaptureFrameRate", &[Value::Int(2)]),
        Value::string("60.00,30.00")
    );
    assert_eq!(
        call_global(&mut game, "CaptureFrameRate", &[Value::Int(0)]),
        Value::string("")
    );
    assert!(has_log(&game, "positive frame count"));
    assert_eq!(
        call_global(&mut game, "CaptureFrameRate", &[Value::Int(3)]),
        Value::string("")
    );
    assert!(has_log(&game, "fewer recent frames"));
    assert_eq!(
        call_global(&mut game, "CaptureFrameRate", &[Value::Float(1.0)]),
        Value::string("")
    );
    assert!(has_log(&game, "requires an integer frame count"));

    for _ in 0..120 {
        game.tick(0.0001);
    }
    assert_eq!(
        call_global(&mut game, "CaptureFrameRate", &[Value::Int(120)]),
        Value::string("")
    );
    assert!(has_log(&game, "exceeds 1,000 characters"));
}

#[test]
fn scr_49_frame_rate_capture_lifecycle_controls_average_and_errors() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(
        call_global(&mut game, "GetAverageFrameRate", &[]),
        Value::Float(0.0)
    );
    assert!(has_log(&game, "has no captured frames"));
    assert_eq!(
        call_global(&mut game, "GetMinFrameRate", &[]),
        Value::Float(0.0)
    );
    assert!(has_log(&game, "GetMinFrameRate has no captured frames"));
    assert_eq!(
        call_global(&mut game, "GetMaxFrameRate", &[]),
        Value::Float(0.0)
    );
    assert!(has_log(&game, "GetMaxFrameRate has no captured frames"));

    assert_eq!(
        call_global(&mut game, "StartFrameRateCapture", &[]),
        Value::None
    );
    game.tick(1.0 / 60.0);
    game.tick(1.0 / 30.0);
    assert_eq!(
        call_global(&mut game, "GetAverageFrameRate", &[]),
        Value::Float(45.0)
    );
    assert_eq!(
        call_global(&mut game, "GetMinFrameRate", &[]),
        Value::Float(30.0)
    );
    assert_eq!(
        call_global(&mut game, "GetMaxFrameRate", &[]),
        Value::Float(60.0)
    );
    assert_eq!(
        call_global(&mut game, "StartFrameRateCapture", &[]),
        Value::None
    );
    assert!(has_log(&game, "while capture is active"));

    assert_eq!(
        call_global(&mut game, "EndFrameRateCapture", &[]),
        Value::None
    );
    assert_eq!(
        call_global(&mut game, "GetAverageFrameRate", &[]),
        Value::Float(45.0)
    );
    assert_eq!(
        call_global(&mut game, "EndFrameRateCapture", &[]),
        Value::None
    );
    assert!(has_log(&game, "without an active capture"));

    call_global(&mut game, "StartFrameRateCapture", &[]);
    game.tick(1.0 / 20.0);
    call_global(&mut game, "EndFrameRateCapture", &[]);
    assert_eq!(
        call_global(&mut game, "GetAverageFrameRate", &[]),
        Value::Float(20.0)
    );
}
