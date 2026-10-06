// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_18_installed_whiterun_noon_night_sky_capture_and_host_controls() {
    run("rnd_18_sky_test.gd", "RND-18 RESULT: PASS");
}

#[test]
fn rnd_18_walkthrough_shared_world_interior_smoke() {
    run("rnd_18_walkthrough_test.gd", "RND-18 WALKTHROUGH: PASS");
}

#[test]
fn rnd_18_benchmark_uses_exterior_weather_environment_and_sun() {
    run("rnd_18_bench_test.gd", "RND-18 BENCH: PASS");
}

fn run(script: &str, marker: &str) {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("Godot binary for installed sky capture");
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let capture = project.join("../target/rnd_18/walkthrough.png");
    std::fs::create_dir_all(project.join("../target/rnd_18")).expect("create capture directory");
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args(["--resolution", "640x360", "--script", script])
        .env("SKYRIM_DATA", data)
        .env("SOE_CAPTURE_PATH", capture)
        .env("SOE_BENCH_CELL", "WhiterunOrigin")
        .env("SOE_BENCH_RADIUS", "0")
        .output()
        .expect("start the RND-18 windowed capture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains(marker),
        "RND-18 host capture failed\n{stdout}\n{stderr}"
    );
    println!("{stdout}");
}
