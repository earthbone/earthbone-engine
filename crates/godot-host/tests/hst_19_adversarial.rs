// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn hst_19_rendered_papyrus_request_writes_png_to_requested_path() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping HST-19 host adversarial check");
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN")
        .map(PathBuf::from)
        .expect("SOE_GODOT_BIN must point to the provided Godot runtime");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(workspace.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "hst_19_screenshot_capture_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("rendered Godot screenshot check starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "rendered HST-19 screenshot check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("RESULT: PASS hst_19 screenshot capture"),
        "rendered HST-19 check did not report success\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("SCRIPT ERROR"),
        "rendered HST-19 check reported a script error\n{stderr}"
    );
}
