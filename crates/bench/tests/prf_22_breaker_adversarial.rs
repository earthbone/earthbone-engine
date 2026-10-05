// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

use std::time::Duration;

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::player_input::session::Session;
use soe_game::player_input::{Command as Input, Driver};
use soe_game::Game;
use soe_papyrus::asm::{f, i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::NativeReturn;
use soe_papyrus::{Opcode, Value};
use soe_trace::Profile;

fn profile_fixture(slow: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 7).sub(Tag(*b"ACBS"), [0; 24]));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), 0x14)
            .sub(Tag(*b"NAME"), 7u32.to_le_bytes())
            .sub(Tag(*b"DATA"), [0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("Replay.esm", plugin).unwrap();
    let mut game = Game::new(order);
    assert!(game.vm.natives.register_global(
        "ReplayProbe",
        "Delay",
        Box::new(move |_| {
            if slow {
                std::thread::sleep(Duration::from_millis(5));
            }
            NativeReturn::Value(Value::None)
        })
    ));
    for name in ["SlowScript", "FastScript"] {
        let mut function = FunctionBuilder::new("Run", "None")
            .global()
            .local("unused", "None");
        if name == "SlowScript" {
            function = function.ins(
                Opcode::CallStatic,
                vec![s("ReplayProbe"), s("Delay"), ident("unused")],
            );
        }
        function = function
            .ins(
                Opcode::CallStatic,
                vec![s("Utility"), s("Wait"), ident("unused"), f(0.1)],
            )
            .ins(
                Opcode::Jmp,
                vec![i(if name == "SlowScript" { -2 } else { -1 })],
            )
            .ins(Opcode::Return, vec![none()]);
        game.load_script(
            &ScriptBuilder::new(name, "")
                .function(function.build())
                .bytes(),
        )
        .unwrap();
        game.vm.set_script_plugin(name, "Replay.esm");
        game.vm.call_global(name, "Run", &[]).unwrap();
    }
    game
}

fn run(before: &serde_json::Value, after: &serde_json::Value) -> std::process::Output {
    let directory = std::env::temp_dir().join(format!(
        "prf-22-breaker-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed")
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let before_path = directory.join("before.json");
    let after_path = directory.join("after.json");
    std::fs::write(&before_path, before.to_string()).unwrap();
    std::fs::write(&after_path, after.to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .arg("profile-diff")
        .arg(&before_path)
        .arg(&after_path)
        .output()
        .unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    output
}

#[test]
fn prf_22_replay_replaces_current_state_with_the_recorded_initial_snapshot() {
    let mut source = Game::new(LoadOrder::new());
    let mut recorder = Driver::default();
    recorder.start_recording(&mut source, u64::MAX).unwrap();
    let initial = source.state_hash();
    recorder.push(Input::Wait).unwrap();
    recorder.step(&mut source).unwrap();
    let session =
        Session::from_bytes(&recorder.finish_recording(&source).unwrap().to_bytes()).unwrap();

    let mut target = Game::new(LoadOrder::new());
    target.world_mut().clock.game_days += 1.0;
    let mut driver = Driver::default();
    assert_ne!(target.state_hash().parts, initial.parts);
    driver.start_replay(&mut target, &session).unwrap();
    assert_eq!(target.state_hash().parts, initial.parts);
    assert_eq!(driver.pending(), 1);
}

#[test]
fn prf_22_empty_recording_restores_state_and_does_not_claim_the_driver() {
    let mut source = Game::new(LoadOrder::new());
    source.world_mut().clock.game_days = 2.0;
    let mut recorder = Driver::default();
    recorder.start_recording(&mut source, 0).unwrap();
    let session =
        Session::from_bytes(&recorder.finish_recording(&source).unwrap().to_bytes()).unwrap();

    let mut target = Game::new(LoadOrder::new());
    let mut driver = Driver::default();
    driver.start_replay(&mut target, &session).unwrap();
    assert_eq!(target.state_hash().parts, source.state_hash().parts);
    assert!(driver.step(&mut target).is_none());
    driver.start_recording(&mut target, 5).unwrap();
    assert!(driver.finish_recording(&target).is_ok());
}

#[test]
fn prf_22_diff_sums_repeated_phases_and_keeps_system_domains_separate() {
    let output = run(
        &serde_json::json!({"traceEvents": [
            {"ph":"C","cat":"actor","name":"A.tick","args":{"entity":"A","self_ns":100,"calls":1}},
            {"ph":"C","cat":"actor","name":"A.move","args":{"entity":"A","self_ns":200,"calls":2}},
            {"ph":"C","cat":"system","name":"vm.game","args":{"entity":"vm","phase":"game","self_ns":50,"calls":1}},
            {"ph":"C","cat":"system","name":"vm.host","args":{"entity":"vm","phase":"host","self_ns":80,"calls":1}}
        ]}),
        &serde_json::json!({"traceEvents": [
            {"ph":"C","cat":"actor","name":"A.tick","args":{"entity":"A","self_ns":110,"calls":1}},
            {"ph":"C","cat":"actor","name":"A.move","args":{"entity":"A","self_ns":250,"calls":3}},
            {"ph":"C","cat":"system","name":"vm.game","args":{"entity":"vm","phase":"game","self_ns":55,"calls":2}},
            {"ph":"C","cat":"system","name":"vm.host","args":{"entity":"vm","phase":"host","self_ns":90,"calls":1}}
        ]}),
    );
    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("\"A\" 300 360 +60 +20.00%"), "{report}");
    assert!(report.contains("\"game.vm\" 50 55 +5 +10.00%"), "{report}");
    assert!(report.contains("\"host.vm\" 80 90 +10 +12.50%"), "{report}");
}

#[test]
fn prf_22_diff_preserves_u64_extremes_and_signed_decreases() {
    let output = run(
        &serde_json::json!({"traceEvents": [
            {"ph":"C","cat":"actor","name":"Gone.tick","args":{"entity":"Gone","self_ns":18446744073709551615u64,"calls":1}},
            {"ph":"C","cat":"actor","name":"Same.tick","args":{"entity":"Same","self_ns":18446744073709551614u64,"calls":1}}
        ]}),
        &serde_json::json!({"traceEvents": [
            {"ph":"C","cat":"actor","name":"Same.tick","args":{"entity":"Same","self_ns":18446744073709551615u64,"calls":1}},
            {"ph":"C","cat":"actor","name":"New.tick","args":{"entity":"New","self_ns":1,"calls":1}}
        ]}),
    );
    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(
        report.contains("\"Gone\" 18446744073709551615 0 -18446744073709551615 -100.00%"),
        "{report}"
    );
    assert!(
        report.contains("\"New\" 0 1 +1 new (baseline 0)"),
        "{report}"
    );
    assert!(
        report.contains("\"Same\" 18446744073709551614 18446744073709551615 +1 +0.00%"),
        "{report}"
    );
}

#[test]
fn prf_22_diff_rejects_scope_traces_and_invalid_counter_values() {
    for event in [
        serde_json::json!({"ph":"X","cat":"actor","name":"A.tick","args":{"self_ns":1,"calls":1}}),
        serde_json::json!({"ph":"C","cat":"actor","name":"A.tick","args":{"self_ns":-1,"calls":1}}),
        serde_json::json!({"ph":"C","cat":"actor","name":"A.tick","args":{"self_ns":1}}),
    ] {
        let output = run(
            &serde_json::json!({"traceEvents":[event]}),
            &serde_json::json!({"traceEvents":[]}),
        );
        assert!(!output.status.success());
    }
}

#[test]
fn prf_22_600_frame_replay_hashes_match_and_slow_script_leads_diff() {
    const MIN_DELAY_NS: u64 = 100_000_000;
    let directory = std::env::temp_dir().join(format!("prf-22-full-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    Profile::disable();
    let mut original_game = profile_fixture(false);
    let mut driver = Driver::default();
    driver.start_recording(&mut original_game, 0x5eed).unwrap();
    Profile::clear();
    Profile::enable();
    for frame in 0..600 {
        driver
            .push(Input::Turn {
                rotation: [0.0, 0.0, frame as f32 / 600.0],
            })
            .unwrap();
        driver.step(&mut original_game).unwrap();
    }
    Profile::disable();
    let session =
        Session::from_bytes(&driver.finish_recording(&original_game).unwrap().to_bytes()).unwrap();
    let before = directory.join("before.json");
    Profile::export_summary(&before).unwrap();

    for slow in [false, true] {
        let mut replay = profile_fixture(slow);
        let mut driver = Driver::default();
        driver.start_replay(&mut replay, &session).unwrap();
        Profile::clear();
        Profile::enable();
        for frame in 1..=600 {
            driver.step(&mut replay).unwrap();
            assert!(driver.replay_error().is_none(), "frame {frame}");
        }
        Profile::disable();
        assert_eq!(replay.state_hash().parts, original_game.state_hash().parts);
    }

    let slow_total: u64 = Profile::ranking(&Profile::summary(), "script")
        .into_iter()
        .find(|(name, _)| name == "SlowScript")
        .unwrap()
        .1
        .self_ns;
    assert!(slow_total >= MIN_DELAY_NS, "slow total {slow_total}");
    let after = directory.join("after.json");
    Profile::export_summary(&after).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .arg("profile-diff")
        .arg(&before)
        .arg(&after)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    let scripts = report.split("\nscript:\n").nth(1).unwrap();
    assert!(
        scripts
            .lines()
            .nth(1)
            .unwrap()
            .trim_start()
            .starts_with("\"SlowScript\""),
        "{report}"
    );
    std::fs::remove_dir_all(directory).unwrap();
}
