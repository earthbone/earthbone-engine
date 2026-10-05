// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! RND-21 production lighting ownership and installed windowed checks.

use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Copy)]
enum Scene {
    Interior,
    Exterior,
}

#[test]
fn rnd_21_walkthrough_and_bench_delegate_all_lighting_to_record_controller() {
    for source in [
        include_str!("../../../godot/walkthrough.gd"),
        include_str!("../../../godot/bench.gd"),
    ] {
        assert!(!source.contains("DirectionalLight3D.new()"));
        assert!(!source.contains("WorldEnvironment.new()"));
        assert!(!source.contains("Environment.new()"));
        assert!(source.contains("res://record_sky.gd"));
    }
}

#[test]
fn rnd_21_installed_walkthrough_replaces_cell_lights_with_weather_sun_and_back() {
    run(
        "rnd_21_walkthrough_test.gd",
        "RND-21 WALKTHROUGH: PASS",
        Scene::Interior,
    );
}

#[test]
fn rnd_21_installed_bench_interior_uses_only_record_cell_lighting() {
    run(
        "rnd_21_bench_test.gd",
        "RND-21 BENCH: PASS",
        Scene::Interior,
    );
}

#[test]
fn rnd_21_installed_bench_exterior_sun_follows_weather_and_time() {
    run(
        "rnd_21_bench_test.gd",
        "RND-21 BENCH: PASS",
        Scene::Exterior,
    );
}

fn run(script: &str, marker: &str, scene: Scene) {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let (cell, radius) = match scene {
        Scene::Interior => ("WhiterunBanneredMare", ""),
        Scene::Exterior => ("WhiterunOrigin", "0"),
    };
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args(["--resolution", "640x360", "--script", script])
        .env("SKYRIM_DATA", data)
        .env("SOE_BENCH_CELL", cell)
        .env("SOE_BENCH_RADIUS", radius)
        .env(
            "SOE_CAPTURE_PATH",
            project.join("../target/rnd_21_walkthrough.png"),
        )
        .env_remove("SOE_AUD_11_TEST")
        .env_remove("SOE_BENCH_SOAK")
        .env_remove("SOE_BENCH_CAPTURE")
        .output()
        .expect("run RND-21 production lighting check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains(marker),
        "RND-21 lighting check failed\n{stdout}\n{stderr}"
    );
    println!("{stdout}");
}
