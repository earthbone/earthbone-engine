// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn hst_19_windowed_godot_saves_the_papyrus_requested_frame() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: set SKYRIM_DATA to the installed Skyrim Data folder");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: set SOE_GODOT_BIN to Godot 4.4 or newer");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg("hst_19_screenshot_capture_test.gd")
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot screenshot check starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Godot HST-19 screenshot check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("RESULT: PASS hst_19 screenshot capture"),
        "Godot HST-19 screenshot check did not report success\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("SCRIPT ERROR"),
        "Godot HST-19 screenshot check reported a script error\n{stderr}"
    );
}
