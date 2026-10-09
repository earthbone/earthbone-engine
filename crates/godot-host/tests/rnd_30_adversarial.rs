// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

static CAPTURE: Mutex<()> = Mutex::new(());

#[test]
fn rnd_30_sky_pass_stays_behind_geometry_and_follows_camera_translation() {
    run_capture("depth");
}

#[test]
fn rnd_30_sky_projection_has_no_sampled_cube_edge_or_horizon_discontinuity() {
    run_capture("edges");
}

#[test]
fn rnd_30_installed_whiterun_has_separate_mountain_and_sky_pixels() {
    run_capture("installed");
}

fn run_capture(case: &str) {
    if case == "installed" && std::env::var_os("SKYRIM_DATA").is_none() {
        eprintln!("SKYRIM_DATA unset, skipping installed Whiterun capture");
        return;
    }
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipping Godot capture");
        return;
    };
    let _guard = CAPTURE.lock().expect("serialize Godot captures");
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = Command::new(godot)
        .arg("--path")
        .arg(project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_30_sky_capture_test.gd",
        ])
        .env("SOE_RND30_CASE", case)
        .output()
        .expect("start RND-30 Godot capture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("RND-30 RESULT: PASS")
            && !stderr.contains("SCRIPT ERROR:")
            && !stderr.contains("SHADER ERROR:"),
        "RND-30 {case} failed\n{stdout}\n{stderr}"
    );
}
