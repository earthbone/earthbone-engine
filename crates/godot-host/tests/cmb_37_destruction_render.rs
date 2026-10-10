// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn cmb_37_windowed_reference_damage_replaces_its_model() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping CMB-37 host model replacement check");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("SOE_GODOT_BIN not set, skipping CMB-37 host model replacement check");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let extension = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"))
        .join("release/libsoe_godot_host.so");
    if !extension.exists() {
        eprintln!("release extension missing, skipping CMB-37 host model replacement check");
        return;
    }
    let output = Command::new(godot)
        .args(["--path"])
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg("cmb_37_destruction_render_test.gd")
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot starts for the CMB-37 windowed replacement check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("RESULT: PASS CMB-37 destruction render")
            && stdout.contains("CMB_37_RENDER")
            && stdout.contains("instances="),
        "CMB-37 windowed model replacement failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
