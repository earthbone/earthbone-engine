// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Installed Whiterun waterfall capture through the production material path.

use std::{path::PathBuf, process::Command};

#[test]
fn rnd_39_installed_whiterun_waterfall_scroll_and_translucency_capture() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("Godot for RND-39 capture");
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    assert!(
        project.join(".godot/rnd_39/placements.json").is_file(),
        "run rnd_39_installed_whiterun_waterfall_materials_and_controllers first"
    );
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_39_waterfall_capture.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("start RND-39 capture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::write(
        project.join(".godot/rnd_39/capture.log"),
        format!("{stdout}\n{stderr}"),
    )
    .expect("write capture log");
    assert!(
        output.status.success() && stdout.contains("rnd_39_waterfall_capture: PASS"),
        "RND-39 capture failed\n{stdout}\n{stderr}"
    );
    println!("{stdout}");
}
