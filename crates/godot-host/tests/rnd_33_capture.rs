// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! RND-33 installed Bannered Mare hearth capture.

use std::{path::PathBuf, process::Command};

#[test]
fn rnd_33_installed_bannered_mare_hearth_capture_matches_its_palette() {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "res://rnd_33_hearth_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("run RND-33 installed hearth capture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("RESULT: PASS rnd_33_bannered_mare_hearth_capture"),
        "RND-33 hearth capture failed\n{stdout}\n{stderr}"
    );
    println!("{stdout}");
}
