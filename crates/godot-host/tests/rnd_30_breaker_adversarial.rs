// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Windowed sky depth, continuity, and installed Whiterun captures.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_30_sky_pass_depth_hemisphere_and_installed_view_hold() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipping Godot capture");
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");

    for case in ["depth", "edges", "installed"] {
        if case == "installed" && std::env::var_os("SKYRIM_DATA").is_none() {
            eprintln!("SKYRIM_DATA unset, skipping installed Whiterun capture");
            continue;
        }
        let output = Command::new(&godot)
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
