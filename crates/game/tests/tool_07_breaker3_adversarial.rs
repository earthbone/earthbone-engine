// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::{Game, PapyrusLogSettings};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::CallStatic;

#[test]
fn tool_07_breaker3_open_user_log_reports_duplicate_and_reopen_results() {
    let root = std::env::temp_dir().join(format!(
        "soe-tool-07-breaker3-{}-{}",
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

    let function = FunctionBuilder::new("Run", "None")
        .global()
        .local("result", "Var")
        .ins(
            CallStatic,
            vec![s("Debug"), s("OpenUserLog"), ident("result"), s("Probe")],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("Trace"), ident("result"), s("first")],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("OpenUserLog"), ident("result"), s("Probe")],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("Trace"), ident("result"), s("duplicate")],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("CloseUserLog"), ident("result"), s("Probe")],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("OpenUserLog"), ident("result"), s("Probe")],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("Trace"), ident("result"), s("reopened")],
        );
    game.load_script(
        &ScriptBuilder::new("Tool07OpenResultProbe", "Form")
            .function(function.build())
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global("Tool07OpenResultProbe", "Run", &[])
        .unwrap();
    game.run_until_idle(32);

    let script_log = std::fs::read_to_string(root.join("Logs/Script/Papyrus.0.log")).unwrap();
    let values: Vec<_> = script_log
        .lines()
        .filter_map(|line| line.rsplit_once("] ").map(|(_, text)| text))
        .filter(|text| matches!(text.to_ascii_lowercase().as_str(), "true" | "false"))
        .map(str::to_ascii_lowercase)
        .collect();
    assert_eq!(
        values,
        ["true", "false", "true"],
        "open result contract: {script_log}"
    );
    assert!(matches!(
        game.vm.thread_result(thread),
        Some(soe_papyrus::Value::None)
    ));
    assert!(!root.join("Logs/Script/User/Probe.0.log").exists());

    let _ = std::fs::remove_dir_all(root);
}
