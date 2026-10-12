// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! SCR-41 proves forced ragdoll state survives rebuilding player collision.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn scr_41_forced_ragdoll_survives_player_collision_rebuild() {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA").map(PathBuf::from),
        std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_GODOT_BIN unset, skipping SCR-41 host check");
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = Command::new(godot)
        .arg("--path")
        .arg(project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "scr_41_ragdoll_collision_retention_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("launch SCR-41 host collision rebuild scenario");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("SCR-41 PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{stdout}\n{stderr}"
    );
}
