// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

fn configured_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

#[test]
fn anm_58_godot_animation_signal_reaches_game_world() {
    let Some(data) = configured_path("SKYRIM_DATA") else {
        eprintln!("skipping: set SKYRIM_DATA to the installed Skyrim Data folder");
        return;
    };
    let Some(godot) = configured_path("SOE_GODOT_BIN") else {
        eprintln!("skipping: set SOE_GODOT_BIN to Godot 4.4 or newer");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let target = configured_path("CARGO_TARGET_DIR").unwrap_or_else(|| workspace.join("target"));
    let extension = target.join("release/libsoe_godot_host.so");
    if !extension.exists() {
        eprintln!("skipping: build the release GDExtension before running this test");
        return;
    }

    let output = Command::new(godot)
        .args([
            "--path",
            workspace
                .join("godot")
                .to_str()
                .expect("Godot project path"),
            "--script",
        ])
        .arg(workspace.join("godot/anm_58_signal_test.gd"))
        .args(["--resolution", "640x360"])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS anm_58 signal bridge"),
        "Godot ANM-58 signal test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
