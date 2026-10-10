// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! RND-25 installed weather cloud scroll-rate check.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_25_cloud_uv_rate_uses_the_weather_speed_scale() {
    let source = include_str!("../../../godot/record_sky.gd");
    assert!(source.contains("const CLOUD_UV_SPEED_SCALE := 1.0 / 400.0"));
    assert!(source.contains("velocities.append(cloud.speed * CLOUD_UV_SPEED_SCALE)"));
    let shader = include_str!("../../../godot/record_sky.gdshader");
    assert!(shader.contains("uv + cloud_speeds[i] * elapsed"));
}

#[test]
fn rnd_25_installed_skyrimcloudy_cloud_rate_matches_its_weather_record() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = Command::new(godot)
        .args([
            "--path",
            project.to_str().expect("Godot project path is UTF-8"),
            "--resolution",
            "640x360",
            "--script",
            "rnd_25_cloud_speed_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("run RND-25 installed cloud speed check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RND-25 RESULT: PASS"),
        "RND-25 cloud speed check failed\n{stdout}\n{stderr}"
    );
    println!("{stdout}");
}
