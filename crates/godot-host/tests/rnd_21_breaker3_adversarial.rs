// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! RND-21 walkthrough round-trip must select weather sun outside interiors.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_21_walkthrough_exterior_uses_weather_sun_after_bannered_mare() {
    run_scene("rnd_21_walkthrough_test.gd", "RND-21 WALKTHROUGH: PASS");
}

#[test]
fn rnd_21_bannered_mare_capture_respects_brightness_and_white_clipping_bounds() {
    run_scene("rnd_21_capture_test.gd", "RND-21 average_linear_luminance=");
}

#[test]
fn rnd_21_benchmark_uses_record_lighting_for_bannered_mare() {
    run_scene("rnd_21_bench_test.gd", "RND-21 BENCH: PASS");
}

fn run_scene(script: &str, marker: &str) {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_GODOT_BIN missing, skipping installed rendering check");
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args(["--resolution", "640x360", "--script", script])
        .env("SKYRIM_DATA", data)
        .env("SOE_BENCH_CELL", "WhiterunBanneredMare")
        .env("SOE_BENCH_RADIUS", "")
        .output()
        .expect("run installed RND-21 rendering check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains(marker),
        "installed rendering check failed\n{stdout}\n{stderr}"
    );
}
