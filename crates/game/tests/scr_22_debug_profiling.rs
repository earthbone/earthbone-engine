// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use std::path::PathBuf;

fn profile_dir(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("soe-scr22-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn worker(script: &str) -> Vec<u8> {
    ScriptBuilder::new(script, "")
        .function(
            FunctionBuilder::new("Work", "None")
                .global()
                .local("value", "Int")
                .ins(IAdd, vec![ident("value"), i(4), i(9)])
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes()
}

fn profiling_probe(start: &str, stop: &str, script_name: Option<&str>) -> Vec<u8> {
    let mut run = FunctionBuilder::new("Run", "None").global();
    let mut start_args = vec![s("Debug"), s(start), ident("::nonevar")];
    let mut stop_args = vec![s("Debug"), s(stop), ident("::nonevar")];
    if let Some(name) = script_name {
        start_args.push(s(name));
        stop_args.push(s(name));
    }
    run = run
        .ins(CallStatic, start_args)
        .ins(
            CallStatic,
            vec![s("ProfileWorker"), s("Work"), ident("::nonevar")],
        )
        .ins(
            CallStatic,
            vec![s("OtherWorker"), s("Work"), ident("::nonevar")],
        )
        .ins(CallStatic, stop_args)
        .ins(Return, vec![none()]);
    ScriptBuilder::new("ProfileProbe", "")
        .function(run.build())
        .bytes()
}

fn run_profile(start: &str, stop: &str, script_name: Option<&str>, tag: &str) -> (PathBuf, String) {
    let dir = profile_dir(tag);
    let mut game = Game::new(LoadOrder::new());
    game.vm.set_profile_directory(&dir);
    game.load_script(&worker("ProfileWorker")).unwrap();
    game.load_script(&worker("OtherWorker")).unwrap();
    game.load_script(&profiling_probe(start, stop, script_name))
        .unwrap();
    game.vm.call_global("ProfileProbe", "Run", &[]).unwrap();
    assert!(game.vm.run_until_idle(100));
    let entry = std::fs::read_dir(&dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let json = std::fs::read_to_string(entry).unwrap();
    (dir, json)
}

#[test]
fn scr_22_script_profile_uses_name_argument_and_writes_vm_trace() {
    let (dir, json) = run_profile(
        "StartScriptProfiling",
        "StopScriptProfiling",
        Some("profileworker"),
        "script",
    );
    assert!(json.contains("ProfileWorker.Work"), "{json}");
    assert!(!json.contains("OtherWorker.Work"), "{json}");
    assert!(json.contains("\"calls\":1"), "{json}");
    assert!(json.contains("\"duration_ns\":") && !json.contains("\"duration_ns\":0"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn scr_22_stack_profile_takes_no_arguments_and_writes_vm_trace() {
    let (dir, json) = run_profile("StartStackProfiling", "StopStackProfiling", None, "stack");
    assert!(json.contains("ProfileWorker.Work"), "{json}");
    assert!(json.contains("OtherWorker.Work"), "{json}");
    assert!(json.contains("\"calls\":1"), "{json}");
    assert!(json.contains("\"duration_ns\":") && !json.contains("\"duration_ns\":0"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn scr_22_profile_write_errors_reach_papyrus_log() {
    for (tag, start, stop, name) in [
        (
            "script-error",
            "StartScriptProfiling",
            "StopScriptProfiling",
            Some("ProfileWorker"),
        ),
        (
            "stack-error",
            "StartStackProfiling",
            "StopStackProfiling",
            None,
        ),
    ] {
        let dir = profile_dir(tag);
        std::fs::write(&dir, "not a directory").unwrap();
        let mut game = Game::new(LoadOrder::new());
        game.vm.set_profile_directory(&dir);
        game.load_script(&worker("ProfileWorker")).unwrap();
        game.load_script(&worker("OtherWorker")).unwrap();
        game.load_script(&profiling_probe(start, stop, name))
            .unwrap();
        game.vm.call_global("ProfileProbe", "Run", &[]).unwrap();
        assert!(game.vm.run_until_idle(100));
        assert!(
            game.vm.log.contains("Papyrus profile write failed"),
            "{stop} should report an output error in the Papyrus log"
        );
        assert_eq!(game.vm.log.messages().len(), 1);
        let _ = std::fs::remove_file(dir);
    }
}
