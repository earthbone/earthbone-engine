// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn prf_16_walk_out_reaches_exterior_with_npc_workload() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKIP PRF-16 walk-out: SKYRIM_DATA is unset");
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = Command::new(godot)
        .args([
            "--headless",
            "--path",
            "godot",
            "--script",
            "prf_16_walkthrough_test.gd",
            "--",
            "walk-out",
        ])
        .env("SKYRIM_DATA", data)
        .env("XDG_DATA_HOME", root.join("target/prf-16-walkout-breaker"))
        .current_dir(&root)
        .output()
        .expect("run installed-data walk-out");
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);

    assert!(result.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("SOE_BUDGET {"), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("\"world_editor_id\":\"WhiterunWorld\""),
        "{stdout}\n{stderr}"
    );
}
