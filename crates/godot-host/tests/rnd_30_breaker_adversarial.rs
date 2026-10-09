// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Windowed sky depth, continuity, and installed Whiterun captures.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_30_sky_pass_depth_hemisphere_and_installed_view_hold() {
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must be set");
    let data = std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA must be set");
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");

    for case in ["depth", "edges", "installed"] {
        let output = Command::new(&godot)
            .arg("--path")
            .arg(&project)
            .args([
                "--resolution",
                "640x360",
                "--script",
                "rnd_30_sky_capture_test.gd",
            ])
            .env("SKYRIM_DATA", &data)
            .env("SOE_RND30_CASE", case)
            .output()
            .expect("run Godot sky capture");
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
}
