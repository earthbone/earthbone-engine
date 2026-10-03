// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::{Game, PapyrusLogSettings};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("soe-tool-07-breaker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn game() -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Test.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    game.start();
    game
}

fn configure(game: &mut Game, root: &Path) {
    game.set_papyrus_log_directory(
        root,
        PapyrusLogSettings {
            enabled: true,
            trace_enabled: true,
            debug_information: true,
        },
    )
    .unwrap();
}

fn run_user_trace(game: &mut Game, class: &str, function: FunctionBuilder) {
    game.load_script(
        &ScriptBuilder::new(class, "Form")
            .function(function.build())
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global(class, "Run", &[]).unwrap();
    game.run_until_idle(32);
    assert!(matches!(game.vm.thread_result(thread), Some(Value::None)));
}

#[test]
fn tool_07_breaker_traceuser_survives_close_before_the_vm_slice_flushes() {
    let root = Scratch::new();
    let mut game = game();
    configure(&mut game, root.path());
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .local("result", "Var")
        .ins(
            CallStatic,
            vec![s("Debug"), s("OpenUserLog"), ident("result"), s("Probe")],
        )
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TraceUser"),
                ident("result"),
                s("Probe"),
                s("before close"),
            ],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("CloseUserLog"), ident("result"), s("Probe")],
        );
    run_user_trace(&mut game, "Tool07CloseProbe", function);
    let path = root.path().join("Logs/Script/User/Probe.0.log");
    let output = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        output.contains("before close"),
        "TraceUser output lost on close: {output}"
    );
}

#[test]
fn tool_07_breaker_reopened_user_log_resets_its_read_cursor() {
    let root = Scratch::new();
    let mut game = game();
    configure(&mut game, root.path());
    let first = FunctionBuilder::new("First", "None")
        .global()
        .local("result", "Var")
        .ins(
            CallStatic,
            vec![s("Debug"), s("OpenUserLog"), ident("result"), s("Probe")],
        )
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TraceUser"),
                ident("result"),
                s("Probe"),
                s("first session"),
            ],
        )
        .ins(Return, vec![]);
    let close = FunctionBuilder::new("Close", "None")
        .global()
        .local("result", "Var")
        .ins(
            CallStatic,
            vec![s("Debug"), s("CloseUserLog"), ident("result"), s("Probe")],
        )
        .ins(Return, vec![]);
    let second = FunctionBuilder::new("Second", "None")
        .global()
        .local("result", "Var")
        .ins(
            CallStatic,
            vec![s("Debug"), s("OpenUserLog"), ident("result"), s("Probe")],
        )
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TraceUser"),
                ident("result"),
                s("Probe"),
                s("second session"),
            ],
        )
        .ins(Return, vec![]);
    let script = ScriptBuilder::new("Tool07ReopenProbe", "Form")
        .function(first.build())
        .function(close.build())
        .function(second.build());
    game.load_script(&script.bytes()).unwrap();
    for name in ["First", "Close", "Second"] {
        let thread = game.vm.call_global("Tool07ReopenProbe", name, &[]).unwrap();
        game.run_until_idle(32);
        assert!(matches!(game.vm.thread_result(thread), Some(Value::None)));
    }
    let output = std::fs::read_to_string(root.path().join("Logs/Script/User/Probe.0.log")).unwrap();
    assert!(output.contains("first session"));
    assert!(
        output.contains("second session"),
        "reopened log missed later line: {output}"
    );
}
