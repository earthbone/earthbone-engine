// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed-data host check for atomic restored-game attachment.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn sav_19_restored_player_cell_camera_and_frame_after_atomic_import() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: set SOE_GODOT_BIN to a Godot 4.4 or newer executable");
        return;
    };
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("skipping: set SKYRIM_DATA to the installed Skyrim Data directory");
        return;
    };
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let extension = target.join("release/libsoe_godot_host.so");
    assert!(extension.is_file(), "build the release GDExtension first");

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let script = project.join("sav_19_host_attach_test.gd");
    let save_dir = tempfile::tempdir().unwrap();
    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--script"])
        .arg(script)
        .args(["--resolution", "640x360"])
        .env("SKYRIM_DATA", data)
        .env("SAV_19_SAVE_DIR", save_dir.path())
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS sav_19 host attach"),
        "SAV-19 host attach failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
