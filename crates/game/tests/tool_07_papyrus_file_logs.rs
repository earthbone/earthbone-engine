// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::{Game, PapyrusLogSettings};
use soe_papyrus::asm::{s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::CallStatic;
use soe_papyrus::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("soe-tool-07-{}-{id}", std::process::id()));
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

fn has_vanilla_timestamp(line: &str) -> bool {
    let Some(timestamp) = line
        .strip_prefix('[')
        .and_then(|line| line.split_once("] ").map(|v| v.0))
    else {
        return false;
    };
    let Some((date, time)) = timestamp.split_once(" - ") else {
        return false;
    };
    let date_fields: Vec<_> = date.split('/').collect();
    let clock = time
        .strip_suffix("AM")
        .or_else(|| time.strip_suffix("PM"))
        .unwrap_or_default();
    date_fields.len() == 3
        && date_fields
            .iter()
            .all(|field| field.bytes().all(|b| b.is_ascii_digit()))
        && !clock.is_empty()
        && clock
            .split(':')
            .all(|field| field.bytes().all(|b| b.is_ascii_digit()))
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

#[test]
fn tool_07_script_trace_user_log_and_vm_error_write_vanilla_log_files() {
    let root = Scratch::new();
    let mut game = game();
    game.set_papyrus_log_directory(
        root.path(),
        PapyrusLogSettings {
            enabled: true,
            trace_enabled: true,
            debug_information: true,
        },
    )
    .unwrap();

    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .local("result", "Var")
        .ins(
            CallStatic,
            vec![s("Debug"), s("Trace"), s("result"), s("trace probe")],
        )
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TraceConditional"),
                s("result"),
                s("conditional trace probe"),
                Value::Bool(true),
            ],
        )
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TraceConditional"),
                s("result"),
                s("suppressed conditional probe"),
                Value::Bool(false),
            ],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("TraceStack"), s("result"), s("stack probe")],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("OpenUserLog"), s("result"), s("TOOL-07")],
        )
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TraceUser"),
                s("result"),
                s("TOOL-07"),
                s("user probe"),
            ],
        );
    function = function.ins(
        CallStatic,
        vec![s("MissingScript"), s("MissingFunction"), s("result")],
    );
    game.load_script(
        &ScriptBuilder::new("Tool07Probe", "Form")
            .function(function.build())
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global("Tool07Probe", "Run", &[]).unwrap();
    game.run_until_idle(64);
    assert!(matches!(game.vm.thread_result(thread), Some(Value::None)));

    let script_log =
        std::fs::read_to_string(root.path().join("Logs/Script/Papyrus.0.log")).unwrap();
    assert!(
        has_vanilla_timestamp(script_log.lines().next().unwrap()),
        "missing vanilla date-time prefix: {script_log}"
    );
    assert!(script_log.contains("trace probe"));
    assert!(script_log.contains("conditional trace probe"));
    assert!(!script_log.contains("suppressed conditional probe"));
    assert!(script_log.contains("Stack trace:"));
    assert!(script_log.contains("Tool07Probe.Run"));
    assert!(script_log.contains("error:"));
    assert!(script_log.contains("stack:\n\t"));
    assert!(script_log.contains("[ ].Tool07Probe.Run() - \"Tool07Probe.psc\" Line ?"));
    assert!(
        script_log.contains("Tool07Probe"),
        "VM error lacks script context: {script_log}"
    );
    let user_log =
        std::fs::read_to_string(root.path().join("Logs/Script/User/TOOL-07.0.log")).unwrap();
    assert!(user_log.contains("user probe"));
}

#[test]
fn tool_07_trace_setting_suppresses_trace_lines_but_keeps_errors() {
    let root = Scratch::new();
    let mut game = game();
    game.set_papyrus_log_directory(
        root.path(),
        PapyrusLogSettings {
            enabled: true,
            trace_enabled: false,
            debug_information: false,
        },
    )
    .unwrap();
    game.vm.log.push(0.0, "ordinary trace");
    game.vm
        .log
        .push(0.0, "error: VM failure (Probe.Run line 3)");
    // A scheduler pass drains the VM log into the configured file.
    game.run_until_idle(1);
    let output = std::fs::read_to_string(root.path().join("Logs/Script/Papyrus.0.log")).unwrap();
    assert!(!output.contains("ordinary trace"));
    assert!(output.contains("VM failure"));
    assert!(!output.contains("Probe.Run line 3"));
}

#[test]
fn tool_07_rotation_moves_previous_logs_up_one_slot() {
    let root = Scratch::new();
    let directory = root.path().join("Logs/Script");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("Papyrus.0.log"), "prior zero\n").unwrap();
    std::fs::write(directory.join("Papyrus.1.log"), "prior one\n").unwrap();
    let mut game = game();
    game.set_papyrus_log_directory(
        root.path(),
        PapyrusLogSettings {
            enabled: true,
            trace_enabled: true,
            debug_information: true,
        },
    )
    .unwrap();
    game.vm.log.push(0.0, "new trace");
    game.run_until_idle(1);
    assert_eq!(
        std::fs::read_to_string(directory.join("Papyrus.1.log")).unwrap(),
        "prior zero\n"
    );
    assert_eq!(
        std::fs::read_to_string(directory.join("Papyrus.2.log")).unwrap(),
        "prior one\n"
    );
    assert!(std::fs::read_to_string(directory.join("Papyrus.0.log"))
        .unwrap()
        .contains("new trace"));
}

#[test]
fn tool_07_profile_papyrus_ini_controls_file_logging() {
    use soe_profile::{Ini, Layer, Settings};

    let root = Scratch::new();
    let mut settings = Settings::new();
    settings.apply(
        &Layer::Base,
        &Ini::parse("[Papyrus]\nbEnableLogging=1\nbEnableTrace=0\nbLoadDebugInformation=1\n"),
    );
    let mut game = game();
    game.configure_papyrus_logs(&settings, root.path()).unwrap();
    game.vm.log.push(0.0, "trace hidden");
    game.vm.log.push(0.0, "error: logged failure");
    game.run_until_idle(1);
    let output = std::fs::read_to_string(root.path().join("Logs/Script/Papyrus.0.log")).unwrap();
    assert!(!output.contains("trace hidden"));
    assert!(output.contains("logged failure"));
}
