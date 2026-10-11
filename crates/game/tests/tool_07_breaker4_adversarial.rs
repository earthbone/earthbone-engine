// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::{Game, PapyrusLogSettings};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

#[test]
fn tool_07_breaker4_traceuser_returns_false_for_a_closed_log() {
    let root = std::env::temp_dir().join(format!(
        "soe-tool-07-breaker4-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let mut order = LoadOrder::new();
    order
        .add("Test.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    game.start();
    game.set_papyrus_log_directory(
        &root,
        PapyrusLogSettings {
            enabled: true,
            trace_enabled: true,
            debug_information: true,
        },
    )
    .unwrap();

    let function = FunctionBuilder::new("Run", "Bool")
        .global()
        .local("result", "Var")
        .ins(
            CallStatic,
            vec![s("Debug"), s("TraceUser"), ident("result"), s("Missing"), s("message")],
        )
        .ins(Return, vec![ident("result")]);
    game.load_script(
        &ScriptBuilder::new("Tool07ClosedLogProbe", "Form")
            .function(function.build())
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global("Tool07ClosedLogProbe", "Run", &[])
        .unwrap();
    game.run_until_idle(32);

    assert!(matches!(game.vm.thread_result(thread), Some(Value::Bool(false))));
    assert!(!PathBuf::from(&root)
        .join("Logs/Script/User/Missing.0.log")
        .exists());
    let _ = std::fs::remove_dir_all(root);
}
