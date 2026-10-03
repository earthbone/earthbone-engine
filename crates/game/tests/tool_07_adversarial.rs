// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::{Game, PapyrusLogSettings};
use soe_papyrus::asm::{s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Operand;
use soe_papyrus::Opcode::CallStatic;
use soe_profile::{Ini, Layer, Settings};
use soe_trace::logging::{self, LogLine};
use soe_trace::tracing::{self, Subscriber};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "soe-tool-07-adversarial-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
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

fn settings(logging: bool, trace: bool, debug: bool) -> Settings {
    let mut settings = Settings::new();
    settings.apply(
        &Layer::Base,
        &Ini::parse(&format!(
            "[Papyrus]\nbEnableLogging={}\nbEnableTrace={}\nbLoadDebugInformation={}\n",
            u8::from(logging),
            u8::from(trace),
            u8::from(debug)
        )),
    );
    settings
}

fn captured() -> (
    impl Subscriber + Send + Sync,
    logging::LogControl,
    Arc<Mutex<Vec<LogLine>>>,
) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&lines);
    let (subscriber, control) = logging::subscriber("trace", None, move |line| {
        sink.lock().unwrap().push(line.clone());
    })
    .unwrap();
    (subscriber, control, lines)
}

#[test]
fn tool_07_adversarial_conditional_trace_stack_and_error_stack_follow_ck_contract() {
    let scratch = Scratch::new();
    let mut game = game();
    game.set_papyrus_log_directory(
        scratch.path(),
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
            vec![s("Debug"), s("TraceConditional"), s("result"), s("visible"), Operand::Bool(true)],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("TraceConditional"), s("result"), s("hidden"), Operand::Bool(false)],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("TraceStack"), s("result"), s("stack marker")],
        )
        .ins(
            CallStatic,
            vec![s("MissingScript"), s("MissingFunction"), s("result")],
        );
    game.load_script(
        &ScriptBuilder::new("Tool07StackProbe", "Form")
            .function(function.build())
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global("Tool07StackProbe", "Run", &[]).unwrap();
    game.run_until_idle(32);
    assert!(matches!(game.vm.thread_result(thread), Some(soe_papyrus::Value::None)));

    let output = std::fs::read_to_string(scratch.path().join("Logs/Script/Papyrus.0.log")).unwrap();
    assert!(output.contains("visible"), "true conditional trace missing: {output}");
    assert!(!output.contains("hidden"), "false conditional trace was emitted: {output}");
    assert!(output.contains("stack marker\n"), "TraceStack message missing: {output}");
    assert!(output.contains("Tool07StackProbe.Run()"), "stack frame missing: {output}");
    assert!(output.contains("stack:\n\t"), "VM error stack missing: {output}");
    assert!(output.contains("Line "), "error frame has no line field: {output}");
}

#[test]
fn tool_07_adversarial_trace_user_opens_once_and_records_in_both_destinations() {
    let scratch = Scratch::new();
    let mut game = game();
    game.set_papyrus_log_directory(
        scratch.path(),
        PapyrusLogSettings {
            enabled: true,
            trace_enabled: true,
            debug_information: false,
        },
    )
    .unwrap();
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .local("result", "Var")
        .ins(CallStatic, vec![s("Debug"), s("OpenUserLog"), s("result"), s("Probe")])
        .ins(CallStatic, vec![s("Debug"), s("OpenUserLog"), s("result"), s("Probe")])
        .ins(CallStatic, vec![s("Debug"), s("TraceUser"), s("result"), s("Probe"), s("user payload")])
        .ins(CallStatic, vec![s("Debug"), s("CloseUserLog"), s("result"), s("Probe")])
        .ins(CallStatic, vec![s("Debug"), s("TraceUser"), s("result"), s("Probe"), s("closed payload")]);
    game.load_script(
        &ScriptBuilder::new("Tool07UserProbe", "Form")
            .function(function.build())
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global("Tool07UserProbe", "Run", &[]).unwrap();
    game.run_until_idle(32);
    assert!(matches!(game.vm.thread_result(thread), Some(soe_papyrus::Value::None)));

    let user = std::fs::read_to_string(scratch.path().join("Logs/Script/User/Probe.0.log")).unwrap();
    assert!(user.contains("user payload"), "TraceUser did not reach user file: {user}");
    assert!(!user.contains("closed payload"), "closed log accepted a trace: {user}");
    let papyrus = std::fs::read_to_string(scratch.path().join("Logs/Script/Papyrus.0.log")).unwrap();
    assert!(papyrus.contains("user payload"), "TraceUser missed Papyrus trace target: {papyrus}");
    assert!(papyrus.contains("closed payload"), "closed log error missed Papyrus file: {papyrus}");
}

#[test]
fn tool_07_adversarial_disabled_logging_does_not_create_script_log_files() {
    let scratch = Scratch::new();
    let mut game = game();
    game.configure_papyrus_logs(&settings(false, true, true), scratch.path())
        .unwrap();
    game.vm.log.push(0.0, "must not reach a file");
    game.run_until_idle(1);
    assert!(
        !scratch.path().join("Logs/Script/Papyrus.0.log").exists(),
        "disabled profile logging created Papyrus.0.log"
    );
    assert!(
        !scratch.path().join("Logs/Script/User").exists(),
        "disabled profile logging created User directory"
    );
}

#[test]
fn tool_07_adversarial_vm_lines_reach_tool_06_papyrus_target_when_file_trace_is_off() {
    let scratch = Scratch::new();
    let (subscriber, control, lines) = captured();
    let mut game = game();
    game.set_papyrus_log_directory(
        scratch.path(),
        PapyrusLogSettings {
            enabled: true,
            trace_enabled: false,
            debug_information: false,
        },
    )
    .unwrap();
    tracing::subscriber::with_default(subscriber, || {
        game.vm.log.push(0.0, "trace routed despite file filter");
        game.vm.log.push(0.0, "error: routed failure (Probe.Run line 4)");
        game.run_until_idle(1);
    });
    control.flush_all().unwrap();
    let papyrus_lines = lines.lock().unwrap();
    assert!(papyrus_lines.iter().any(|line| {
        line.target == "papyrus"
            && line.fields.get("message").is_some_and(|message| message == "trace routed despite file filter")
    }));
    assert!(papyrus_lines.iter().any(|line| {
        line.target == "papyrus"
            && line.level == tracing::Level::ERROR
            && line.fields.get("message").is_some_and(|message| message == "error: routed failure (Probe.Run line 4)")
    }));
}
