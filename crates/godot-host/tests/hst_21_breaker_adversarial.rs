// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-21 live installed-data regression for door identity and collision.

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn hst_21_two_installed_doors_keep_identity_without_stale_collision() {
    assert!(std::env::var_os("SKYRIM_DATA").is_some(), "SKYRIM_DATA is required");
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = Command::new(godot)
        .arg("--path")
        .arg(project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "hst_21_resident_doors_test.gd",
        ])
        .output()
        .expect("run the installed Bannered Mare and Breezehome door scene");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS hst_21_resident_doors"),
        "installed HST-21 scene failed\n{stdout}\n{stderr}"
    );
    assert!(Path::new(&std::env::var("SKYRIM_DATA").unwrap()).is_dir());
}
