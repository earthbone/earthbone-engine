// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "godot")]

//! PRF-31 launches the collision-guided walk checks against the prepared host.

use std::path::Path;
use std::process::Command;

#[test]
fn prf_31_walk_route_turns_on_stalls_without_moving_the_player() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = Command::new(godot)
        .arg("--headless")
        .arg("--path")
        .arg(project)
        .args(["--script", "prf_31_walk_route_test.gd"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("RESULT: PASS PRF31 walk route"), "{stdout}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}

#[test]
fn prf_31_installed_windowed_gate_walk_keeps_every_detection_update_under_two_ms() {
    let (Some(godot), Some(data)) = (
        std::env::var_os("SOE_GODOT_BIN"),
        std::env::var_os("SKYRIM_DATA"),
    ) else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(
        root.join("target/release/libsoe_godot_host.so").is_file(),
        "build the release extension and import the Godot project first"
    );
    let output = Command::new(godot)
        .arg("--path")
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "prf_31_detection_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env_remove("PRF_31_BASELINE")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("PRF31 walk seconds=60"), "{stdout}");
    assert!(stdout.contains("over_budget=0"), "{stdout}");
    assert!(stdout.contains("RESULT: PASS PRF31 detection"), "{stdout}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}
