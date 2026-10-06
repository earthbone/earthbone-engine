// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Sky-pass occlusion and seamless hemisphere captures using the Godot sky API.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

static CAPTURE: Mutex<()> = Mutex::new(());

#[test]
fn rnd_30_far_geometry_and_camera_translation() {
    run("depth");
}

#[test]
fn rnd_30_hemisphere_has_no_cube_edges_or_horizon_streaks() {
    run("edges");
}

#[test]
fn rnd_30_installed_whiterun_mountain_and_sky_texel_masks() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        eprintln!("SKYRIM_DATA unset, skipping installed Whiterun capture");
        return;
    }
    run("installed");
}

fn run(case: &str) {
    let _guard = CAPTURE.lock().expect("serialize host captures");
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipping Godot capture");
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    std::fs::create_dir_all(project.join("../target/rnd_30")).expect("create capture directory");
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_30_sky_capture_test.gd",
        ])
        .env("SOE_RND30_CASE", case)
        .output()
        .expect("start windowed sky capture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::write(
        project.join(format!("../target/rnd_30/{case}.log")),
        format!("{stdout}\n{stderr}"),
    )
    .expect("write capture log");
    assert!(
        output.status.success()
            && stdout.contains("RND-30 RESULT: PASS")
            && !stderr.contains("SCRIPT ERROR:")
            && !stderr.contains("SHADER ERROR:"),
        "RND-30 {case} failed\n{stdout}\n{stderr}"
    );
    println!("{stdout}");
}
