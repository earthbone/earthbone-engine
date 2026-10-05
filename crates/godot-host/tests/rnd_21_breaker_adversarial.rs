// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_21_bannered_mare_capture_meets_brightness_and_clipping_bounds() {
    let data = std::env::var("SKYRIM_DATA").expect("SKYRIM_DATA must point at installed game data");
    let godot = std::env::var("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must point at Godot 4.4+");
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let result = Command::new(godot)
        .args([
            "--path",
            project.to_str().unwrap(),
            "--resolution",
            "640x360",
            "--script",
            "res://rnd_21_capture_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("run Bannered Mare capture");
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success() && stdout.contains("RND-21 average_linear_luminance="),
        "capture failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
