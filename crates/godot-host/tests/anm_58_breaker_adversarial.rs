// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn anm_58_godot_signal_wait_event_unload_and_disable() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: set SKYRIM_DATA to the installed Skyrim Data folder");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: set SOE_GODOT_BIN to Godot 4.4 or newer");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let build = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args(["build", "-p", "soe-godot-host", "--release", "--target-dir"])
        .arg(workspace.join("target"))
        .output()
        .expect("release extension build starts");
    assert!(
        build.status.success(),
        "release extension build failed\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(godot)
        .args(["--headless", "--path"])
        .arg(workspace.join("godot"))
        .args(["--script"])
        .arg(workspace.join("godot/anm_58_adversarial_test.gd"))
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot starts the ANM-58 signal test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS anm_58 adversarial"),
        "ANM-58 signal check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
