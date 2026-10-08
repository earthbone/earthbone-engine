// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::{Command, Output};

fn bench(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .args(args)
        .output()
        .expect("soe-bench should start")
}

#[test]
#[ignore = "needs PRF-37: only high-process AI executes until the lower process levels run"]
fn prf_28_each_vanilla_process_level_executes_a_capacity_measurement() {
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
            "{level} process level did not execute: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("CAPACITY_RESULT"));
    }
}

#[test]
#[ignore = "needs PRF-37: the PRF-17 worker pool it builds is absent, so workers other than 1 are rejected"]
fn prf_28_capacity_accepts_one_and_core_count_ai_workers() {
    for workers in [1, std::thread::available_parallelism().unwrap().get()] {
        let workers = workers.to_string();
        let output = bench(&[
            "capacity",
            "ai",
            "synthetic",
            "--workers",
            &workers,
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
            "{workers} workers did not execute: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("CAPACITY_RESULT"));
    }
}

#[test]
fn prf_28_installed_animation_executes_a_measured_capacity_rung() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKIP PRF-28: SKYRIM_DATA is unset");
        return;
    };
    let output = bench(&[
        "capacity",
        "animation",
        "whiterun",
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
        "installed animation failed with data at {}: {}",
        PathBuf::from(data).display(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("CAPACITY_RESULT"));
}

#[test]
fn prf_28_windowed_light_capacity_runs_each_world_and_shadow_mode() {
    let (Some(godot), Some(data)) = (
        std::env::var_os("SOE_GODOT_BIN"),
        std::env::var_os("SKYRIM_DATA"),
    ) else {
        eprintln!("SKIP PRF-28: SOE_GODOT_BIN or SKYRIM_DATA is unset");
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate parent")
        .parent()
        .expect("workspace root")
        .join("godot");
    for (world, lights) in [
        ("synthetic", "dynamic"),
        ("synthetic", "shadow"),
        ("whiterun", "dynamic"),
        ("whiterun", "shadow"),
    ] {
        let output = Command::new(&godot)
            .args([
                "--path",
                project.to_str().expect("Godot project path"),
                "--resolution",
                "640x360",
                "--script",
                "capacity_lights.gd",
            ])
            .env("SKYRIM_DATA", &data)
            .env("SOE_CAPACITY_WORLD", world)
            .env("SOE_CAPACITY_LIGHTS", lights)
            .env("SOE_CAPACITY_MAX", "1")
            .output()
            .expect("Godot light capacity scene should start");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stdout.contains("CAPACITY_RESULT"),
            "{world} {lights} capacity failed: stdout={stdout}\nstderr={stderr}"
        );
    }
}
