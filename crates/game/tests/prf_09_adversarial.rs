// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::Opcode::*;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, i, ident, none, s};
use std::path::PathBuf;

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

#[test]
fn prf_09_script_profile_counts_repeated_matching_function_calls() {
    let dir = std::env::temp_dir().join(format!("soe-prf09-breaker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let probe = FunctionBuilder::new("Run", "None")
        .global()
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("StartScriptProfiling"),
                ident("::nonevar"),
                s("ProfileWorker"),
            ],
        )
        .ins(
            CallStatic,
            vec![s("ProfileWorker"), s("Work"), ident("::nonevar")],
        )
        .ins(
            CallStatic,
            vec![s("ProfileWorker"), s("Work"), ident("::nonevar")],
        )
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("StopScriptProfiling"),
                ident("::nonevar"),
                s("ProfileWorker"),
            ],
        )
        .ins(Return, vec![none()])
        .build();
    let mut game = Game::new(LoadOrder::new());
    game.vm.set_profile_directory(&dir);
    game.load_script(&worker()).unwrap();
    game.load_script(
        &ScriptBuilder::new("ProfileProbe", "")
            .function(probe)
            .bytes(),
    )
    .unwrap();
    game.vm.call_global("ProfileProbe", "Run", &[]).unwrap();
    assert!(game.run_until_idle(100));
    let path: PathBuf = std::fs::read_dir(&dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let json = std::fs::read_to_string(path).unwrap();
    assert!(json.contains("ProfileWorker.Work"));
    assert!(json.contains("\"calls\":2"));
    assert!(json.contains("\"duration_ns\":") && !json.contains("\"duration_ns\":0"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn prf_09_starting_profile_inside_target_script_counts_its_call_once() {
    let dir = std::env::temp_dir().join(format!("soe-prf09-self-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let run = FunctionBuilder::new("Run", "None")
        .global()
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("StartScriptProfiling"),
                ident("::nonevar"),
                s("SelfProfile"),
            ],
        )
        .ins(Return, vec![none()])
        .build();
    let stop = FunctionBuilder::new("Stop", "None")
        .global()
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("StopScriptProfiling"),
                ident("::nonevar"),
                s("SelfProfile"),
            ],
        )
        .ins(Return, vec![none()])
        .build();
    let mut game = Game::new(LoadOrder::new());
    game.vm.set_profile_directory(&dir);
    game.load_script(
        &ScriptBuilder::new("SelfProfile", "")
            .function(run)
            .function(stop)
            .bytes(),
    )
    .unwrap();
    game.vm.call_global("SelfProfile", "Run", &[]).unwrap();
    game.vm.call_global("SelfProfile", "Stop", &[]).unwrap();
    assert!(game.run_until_idle(100));
    let path = std::fs::read_dir(&dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let json = std::fs::read_to_string(path).unwrap();
    assert!(json.contains("\"name\":\"SelfProfile.Run\""));
    assert!(json.contains("\"calls\":1"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn prf_09_stack_profile_counts_the_starting_frame_once() {
    let dir = std::env::temp_dir().join(format!("soe-prf09-stack-self-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let run = FunctionBuilder::new("Run", "None")
        .global()
        .ins(
            CallStatic,
            vec![s("Debug"), s("StartStackProfiling"), ident("::nonevar")],
        )
        .ins(
            CallStatic,
            vec![s("ProfileWorker"), s("Work"), ident("::nonevar")],
        )
        .ins(
            CallStatic,
            vec![s("Debug"), s("StopStackProfiling"), ident("::nonevar")],
        )
        .ins(Return, vec![none()])
        .build();
    let mut game = Game::new(LoadOrder::new());
    game.vm.set_profile_directory(&dir);
    game.load_script(&worker()).unwrap();
    game.load_script(
        &ScriptBuilder::new("StackProfileProbe", "")
            .function(run)
            .bytes(),
    )
    .unwrap();
    game.vm
        .call_global("StackProfileProbe", "Run", &[])
        .unwrap();
    assert!(game.run_until_idle(100));
    let path = std::fs::read_dir(&dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let json = std::fs::read_to_string(path).unwrap();
    assert!(json.contains("StackProfileProbe.Run"));
    assert!(json.contains("ProfileWorker.Work"));
    assert!(json.contains("\"calls\":1"));
    let _ = std::fs::remove_dir_all(dir);
}
