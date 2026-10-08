// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::process::{Command, Output};

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn bench(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .args(args)
        .output()
        .expect("soe-bench should start")
}

#[test]
#[ignore = "needs PRF-37: only high-process AI executes until the lower process levels run"]
fn prf_28_measures_each_vanilla_process_level() {
    for level in ["high", "middle_high", "middle_low", "low"] {
        let output = bench(&[
            "capacity",
            "ai",
            "synthetic",
            "--level",
            level,
            "--start",
            "1",
            "--max",
            "1",
            "--samples",
            "100",
            "--warmup",
            "1",
        ]);
        assert!(
            output.status.success(),
            "process level {level} rejected: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("CAPACITY_RESULT"), "no result for {level}");
    }
}

#[test]
#[ignore = "needs PRF-17 worker pool: workers other than 1 are rejected until it lands"]
fn prf_28_measures_ai_with_all_available_workers() {
    let workers = std::thread::available_parallelism().unwrap().get();
    let worker_arg = workers.to_string();
    let output = bench(&[
        "capacity",
        "ai",
        "synthetic",
        "--workers",
        &worker_arg,
        "--start",
        "1",
        "--max",
        "1",
        "--samples",
        "100",
        "--warmup",
        "1",
    ]);
    assert!(
        output.status.success(),
        "{} workers rejected: {}",
        workers,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("CAPACITY_RESULT"));
}

#[test]
fn prf_28_synthetic_animation_reports_a_measured_capacity_curve() {
    let output = bench(&[
        "capacity",
        "animation",
        "synthetic",
        "--start",
        "16",
        "--max",
        "16",
        "--samples",
        "100",
        "--warmup",
        "1",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ANIMATION_WORK bones=99"));
    assert!(stdout.contains("skin_matrices=99"));
    assert!(stdout.contains("CAPACITY_RESULT"));
}

#[test]
fn prf_28_synthetic_ai_reports_a_measured_capacity_curve() {
    let output = bench(&[
        "capacity",
        "ai",
        "synthetic",
        "--start",
        "16",
        "--max",
        "16",
        "--samples",
        "100",
        "--warmup",
        "1",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CAPACITY_SAMPLE"));
    assert!(stdout.contains("CAPACITY_RESULT"));
}

#[test]
fn prf_28_game_settings_use_the_winning_editor_id_override() {
    let mut order = LoadOrder::new();
    for (plugin_name, value) in [("Base.esm", 20i32), ("Raise.esp", 100)] {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(T(b"GMST"), 0x800)
                .zsub(T(b"EDID"), "iAINumberActorsComplexScene")
                .sub(T(b"DATA"), value.to_le_bytes()),
        );
        order.add(plugin_name, plugin).unwrap();
    }
    let world = World::new(order);
    assert_eq!(
        world.game_setting("iAINumberActorsComplexScene"),
        Some(soe_papyrus::Value::Int(100))
    );
    assert_eq!(world.game_setting("iMissingSetting"), None);
}
