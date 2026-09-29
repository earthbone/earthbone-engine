// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use std::path::PathBuf;

fn profile_dir(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("soe-prf09-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn worker() -> Vec<u8> {
    ScriptBuilder::new("ProfileWorker", "")
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

fn probe(start: &str, stop: &str, argument: Option<&str>) -> Vec<u8> {
    let mut run = FunctionBuilder::new("Run", "None").global();
    let mut start_args = vec![s("Debug"), s(start), ident("::nonevar")];
    let mut stop_args = vec![s("Debug"), s(stop), ident("::nonevar")];
    if let Some(name) = argument {
        start_args.push(s(name));
        stop_args.push(s(name));
    }
    run = run
        .ins(CallStatic, start_args)
        .ins(
            CallStatic,
            vec![s("ProfileWorker"), s("Work"), ident("::nonevar")],
        )
        .ins(CallStatic, stop_args)
        .ins(Return, vec![none()]);
    ScriptBuilder::new("ProfileProbe", "")
        .function(run.build())
        .bytes()
}

fn run_profile(start: &str, stop: &str, argument: Option<&str>, tag: &str) -> (PathBuf, String) {
    let dir = profile_dir(tag);
    let mut game = Game::new(LoadOrder::new());
    game.vm.set_profile_directory(&dir);
    game.load_script(&worker()).unwrap();
    game.load_script(&probe(start, stop, argument)).unwrap();
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
fn prf_09_script_profile_writes_worker_call_count_and_timing() {
    let (dir, json) = run_profile(
        "StartScriptProfiling",
        "StopScriptProfiling",
        Some("ProfileWorker"),
        "script",
    );
    assert!(
        json.contains("\"traceEvents\""),
        "profile is Chrome trace JSON"
    );
    assert!(
        json.contains("ProfileWorker.Work"),
        "profile names the target function"
    );
    assert!(
        json.contains("\"calls\":1"),
        "profile records one function call"
    );
    assert!(
        json.contains("\"duration_ns\":") && !json.contains("\"duration_ns\":0"),
        "profile records function timing"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn prf_09_stack_profile_writes_worker_call_count_and_timing() {
    let (dir, json) = run_profile("StartStackProfiling", "StopStackProfiling", None, "stack");
    assert!(
        json.contains("\"traceEvents\""),
        "profile is Chrome trace JSON"
    );
    assert!(
        json.contains("ProfileWorker.Work"),
        "profile names a function on the profiled stack"
    );
    assert!(
        json.contains("\"calls\":1"),
        "profile records one function call"
    );
    assert!(
        json.contains("\"duration_ns\":") && !json.contains("\"duration_ns\":0"),
        "profile records function timing"
    );
    let _ = std::fs::remove_dir_all(dir);
}
