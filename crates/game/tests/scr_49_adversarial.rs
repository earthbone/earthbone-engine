// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::{
    Opcode::{CallStatic, Return},
    Value,
    asm::{FunctionBuilder, ScriptBuilder, ident, s},
};
fn call(g: &mut Game, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr49Edge{name}");
    let mut f = FunctionBuilder::new("Run", "Var")
        .global()
        .local("r", "Var");
    let mut o = vec![s("Utility"), s(name), ident("r")];
    for (n, v) in args.iter().enumerate() {
        let a = format!("a{n}");
        f = f.param(
            &a,
            if matches!(v, Value::Int(_)) {
                "Int"
            } else {
                "Float"
            },
        );
        o.push(ident(&a));
    }
    let b = ScriptBuilder::new(&script, "")
        .function(f.ins(CallStatic, o).ins(Return, vec![ident("r")]).build())
        .bytes();
    g.load_script(&b).unwrap();
    let t = g.vm.call_global(&script, "Run", args).unwrap();
    assert!(g.run_until_idle(32));
    g.vm.thread_result(t).unwrap_or(Value::None)
}
#[test]
fn scr_49_invalid_delta_does_not_enter_capture_or_recent_history() {
    let mut g = Game::new(LoadOrder::new());
    call(&mut g, "StartFrameRateCapture", &[]);
    g.tick(f64::NAN);
    g.tick(-1.);
    g.tick(0.);
    assert_eq!(
        call(&mut g, "CaptureFrameRate", &[Value::Int(1)]),
        Value::string("")
    );
    assert_eq!(call(&mut g, "GetAverageFrameRate", &[]), Value::Float(0.));
    assert!(g.traces().iter().any(|x| x.contains("fewer recent frames")));
}
