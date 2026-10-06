// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Adversarial Godot checks for RND-21 production lighting and capture.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_21_walkthrough_preserves_interior_lighting_across_exterior_round_trip() {
    run_scene("rnd_21_walkthrough_test.gd", "RND-21 WALKTHROUGH: PASS");
}

#[test]
fn rnd_21_bench_restores_interior_after_weather_and_time_changes() {
    run_bench("WhiterunOrigin", "0");
}

#[test]
fn rnd_21_bannered_mare_capture_stays_below_white_clipping_bound() {
    let (Some(data), Some(godot)) = (configured("SKYRIM_DATA"), configured("SOE_GODOT_BIN")) else {
        eprintln!("SKYRIM_DATA or SOE_GODOT_BIN missing, skipping installed capture");
        return;
    };
    let project = project_path();
    let output = Command::new(godot)
        .args([
            "--path",
            project.to_str().expect("Godot project path is UTF-8"),
            "--resolution",
            "640x360",
            "--script",
            "res://rnd_21_capture_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("run installed Bannered Mare capture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RND-21 average_linear_luminance="),
        "capture failed\n{stdout}\n{stderr}"
    );
}

fn run_bench(cell: &str, radius: &str) {
    let (Some(data), Some(godot)) = (configured("SKYRIM_DATA"), configured("SOE_GODOT_BIN")) else {
        eprintln!("SKYRIM_DATA or SOE_GODOT_BIN missing, skipping installed benchmark");
        return;
    };
    let project = project_path();
    let output = Command::new(godot)
        .args([
            "--path",
            project.to_str().expect("Godot project path is UTF-8"),
            "--resolution",
            "640x360",
            "--script",
            "res://rnd_21_bench_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("SOE_BENCH_CELL", cell)
        .env("SOE_BENCH_RADIUS", radius)
        .output()
        .expect("run installed benchmark lighting check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RND-21 BENCH: PASS"),
        "benchmark lighting check failed\n{stdout}\n{stderr}"
    );
}

fn run_scene(script: &str, marker: &str) {
    let (Some(data), Some(godot)) = (configured("SKYRIM_DATA"), configured("SOE_GODOT_BIN")) else {
        eprintln!("SKYRIM_DATA or SOE_GODOT_BIN missing, skipping installed lighting scene");
        return;
    };
    let project = project_path();
    let output = Command::new(godot)
        .args([
            "--path",
            project.to_str().expect("Godot project path is UTF-8"),
            "--resolution",
            "640x360",
            "--script",
            script,
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("run installed walkthrough lighting check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains(marker),
        "lighting scene failed\n{stdout}\n{stderr}"
    );
}

fn configured(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name)
}

fn project_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot")
}
