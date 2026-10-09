// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn hst_19_headless_papyrus_request_writes_png_to_requested_path() {
    let data = std::env::var_os("SKYRIM_DATA")
        .map(PathBuf::from)
        .expect("SKYRIM_DATA must point to the installed Skyrim Data folder");
    let godot = std::env::var_os("SOE_GODOT_BIN")
        .map(PathBuf::from)
        .expect("SOE_GODOT_BIN must point to the provided Godot runtime");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .args(["--headless", "--path"])
        .arg(workspace.join("godot"))
        .args(["--script", "hst_19_screenshot_capture_test.gd"])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("headless Godot screenshot check starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "headless HST-19 screenshot check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("RESULT: PASS hst_19 screenshot capture"),
        "headless HST-19 check did not report success\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("SCRIPT ERROR"),
        "headless HST-19 check reported a script error\n{stderr}"
    );
}
