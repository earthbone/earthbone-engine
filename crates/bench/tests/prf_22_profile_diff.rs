// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-22 records real VM work, replays twice and diffs its PRF-13 exports.

use std::process::Command as Process;
use std::time::Duration;

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::player_input::session::Session;
use soe_game::player_input::{Command, Driver};
use soe_game::Game;
use soe_papyrus::asm::{f, i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::NativeReturn;
use soe_papyrus::{Opcode, Value};
use soe_trace::Profile;

fn fixture(slow: bool) -> Game {
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

#[test]
fn prf_22_600_frame_session_replays_twice_and_injected_slow_script_tops_profile_diff() {
    const MIN_DELAY_NS: u64 = 100_000_000;
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("prf-22-diff-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    Profile::disable();
    let mut game = fixture(false);
    let mut driver = Driver::default();
    driver.start_recording(&mut game, 42).unwrap();
    Profile::clear();
    Profile::enable();
    for frame in 0..600 {
        driver
            .push(Command::Turn {
                rotation: [0.0, 0.0, frame as f32 / 600.0],
            })
            .unwrap();
        driver.step(&mut game).unwrap();
    }
    Profile::disable();
    let session = driver.finish_recording(&game).unwrap();
    let before = directory.join("before.json");
    Profile::export_summary(&before).unwrap();
    let file = directory.join("session.soer");
    session.write(&file).unwrap();
    let session = Session::read(&file).unwrap();
    for slow in [false, true] {
        let mut replay = fixture(slow);
        let mut driver = Driver::default();
        driver.start_replay(&mut replay, &session).unwrap();
        Profile::clear();
        Profile::enable();
        for _ in 0..600 {
            driver.step(&mut replay).unwrap();
            assert!(
                driver.replay_error().is_none(),
                "{:?}",
                driver.replay_error()
            );
        }
        Profile::disable();
        assert_eq!(replay.state_hash().parts, game.state_hash().parts);
    }
    let slow_total: u64 = Profile::ranking(&Profile::summary(), "script")
        .into_iter()
        .find(|(name, _)| name == "SlowScript")
        .unwrap()
        .1
        .self_ns;
    assert!(
        slow_total >= MIN_DELAY_NS,
        "injected script did not run: {slow_total}"
    );
    let after = directory.join("after.json");
    Profile::export_summary(&after).unwrap();
    let output = Process::new(env!("CARGO_BIN_EXE_soe-bench"))
        .arg("profile-diff")
        .arg(&before)
        .arg(&after)
        .env_remove("SKYRIM_DATA")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    assert!(
        report.contains("Replay.esm") && report.contains("game.papyrus_vm") && report.contains('%'),
        "{report}"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn prf_22_profile_diff_cli_lists_all_entity_kinds_counts_and_missing_entities() {
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("prf-22-kinds-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let make = |scale| {
        serde_json::json!({"traceEvents": (["actor","script","quest","plugin","system"].map(|kind| {
            serde_json::json!({"ph":"C","cat":kind,"name":"Entity.Run.phase","args":{"self_ns":100 * scale,"calls":2 * scale,"record_lookups":4 * scale}})
        }))})
    };
    let before = directory.join("before.json");
    let after = directory.join("after.json");
    std::fs::write(&before, make(1).to_string()).unwrap();
    std::fs::write(&after, make(2).to_string()).unwrap();
    let output = Process::new(env!("CARGO_BIN_EXE_soe-bench"))
        .arg("profile-diff")
        .arg(&before)
        .arg(&after)
        .env_remove("SKYRIM_DATA")
        .output()
        .unwrap();
    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    for kind in ["actor", "script", "quest", "plugin", "system"] {
        assert!(report.contains(&format!("\n{kind}:\n")));
    }
    assert!(report.contains("record_lookups:") && report.contains("+100.00%"));
    let output = Process::new(env!("CARGO_BIN_EXE_soe-bench"))
        .arg("profile-diff")
        .arg(&before)
        .output()
        .unwrap();
    assert!(!output.status.success());
    std::fs::remove_dir_all(directory).unwrap();
}
