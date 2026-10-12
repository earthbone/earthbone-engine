// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::{Game, PapyrusLogSettings};
use soe_papyrus::asm::{s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::CallStatic;
use soe_papyrus::Value;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "soe-tool-07-breaker2-{}-{}",
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

fn configure(game: &mut Game, root: &Path, debug_information: bool) {
    game.set_papyrus_log_directory(
        root,
        PapyrusLogSettings {
            enabled: true,
            trace_enabled: true,
            debug_information,
        },
    )
    .unwrap();
}

#[test]
fn tool_07_breaker2_timestamp_uses_two_digit_hour_and_clock_fields() {
    let root = Scratch::new();
    let mut game = game();
    configure(&mut game, root.path(), true);
    game.vm.log.push(0.0, "timestamp probe");
    game.run_until_idle(1);

    let output = std::fs::read_to_string(root.path().join("Logs/Script/Papyrus.0.log")).unwrap();
    let line = output.lines().next().unwrap();
    let timestamp = line.strip_prefix('[').unwrap().split_once("] ").unwrap().0;
    let (date, time) = timestamp.split_once(" - ").unwrap();
    let date: Vec<_> = date.split('/').collect();
    let (clock, ampm) = time.split_at(time.len() - 2);
    let fields: Vec<_> = clock.split(':').collect();
    assert_eq!(date.len(), 3, "invalid vanilla date: {line}");
    assert_eq!(date[0].len(), 2, "month must be two digits: {line}");
    assert_eq!(date[1].len(), 2, "day must be two digits: {line}");
    assert_eq!(date[2].len(), 4, "year must be four digits: {line}");
    assert_eq!(fields.len(), 3, "invalid vanilla clock: {line}");
    assert_eq!(fields[0].len(), 2, "hour must be two digits: {line}");
    assert_eq!(fields[1].len(), 2, "minute must be two digits: {line}");
    assert_eq!(fields[2].len(), 2, "second must be two digits: {line}");
    assert!(matches!(ampm, "AM" | "PM"), "invalid AM/PM suffix: {line}");
}

#[test]
fn tool_07_breaker2_debug_information_selects_error_stack_line_detail() {
    for (enabled, expected) in [(true, "Line 17"), (false, "Line ?")] {
        let root = Scratch::new();
        let mut game = game();
        configure(&mut game, root.path(), enabled);
        game.vm
            .log
            .push(0.0, "error: probe failure (Probe.Run line 17)");
        game.run_until_idle(1);

        let output =
            std::fs::read_to_string(root.path().join("Logs/Script/Papyrus.0.log")).unwrap();
        assert!(
            output.contains(expected),
            "debug_information={enabled} expected {expected}: {output}"
        );
    }
}

#[test]
fn tool_07_breaker2_trace_user_reaches_named_file_when_ordinary_trace_is_disabled() {
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
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .local("result", "Var")
        .ins(
            CallStatic,
            vec![s("Debug"), s("OpenUserLog"), s("result"), s("Probe")],
        )
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("TraceUser"),
                s("result"),
                s("Probe"),
                s("named trace survives file filter"),
            ],
        );
    game.load_script(
        &ScriptBuilder::new("Tool07UserFilterProbe", "Form")
            .function(function.build())
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global("Tool07UserFilterProbe", "Run", &[])
        .unwrap();
    game.run_until_idle(32);
    assert!(matches!(game.vm.thread_result(thread), Some(Value::None)));

    let user = std::fs::read_to_string(root.path().join("Logs/Script/User/Probe.0.log")).unwrap();
    assert!(user.contains("named trace survives file filter"), "{user}");
    let papyrus =
        std::fs::read_to_string(root.path().join("Logs/Script/Papyrus.0.log")).unwrap_or_default();
    assert!(
        !papyrus.contains("named trace survives file filter"),
        "{papyrus}"
    );
}
