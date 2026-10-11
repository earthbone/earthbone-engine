// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! PRF-12 custom monitors, F3 presentation and walkthrough shutdown checks.
//! Scene assertions follow Godot's Performance custom-monitor contract.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

fn run(godot: &OsStr, script: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .args([
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            script,
        ])
        .env(
            "XDG_DATA_HOME",
            root.join("target/prf-12-users").join(script),
        )
        .env("SOE_PRF_12_CAPTURE", root.join("target/prf-12-overlay.png"))
        .current_dir(&root)
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{log}");
    assert!(output.status.success(), "{script}: {log}");
    assert!(!log.contains("SCRIPT ERROR:"), "{log}");
    log
}

#[test]
fn prf_12_godot_monitors_overlay_and_summary() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let log = run(&godot, "prf_12_monitors_test.gd");
    assert!(log.contains("prf_12_godot_monitors_overlay_and_summary: PASS"));
}

#[test]
fn prf_12_installed_walkthrough_runtime_and_exit_summary() {
    let (Some(godot), Some(_data)) = (
        std::env::var_os("SOE_GODOT_BIN"),
        std::env::var_os("SKYRIM_DATA"),
    ) else {
        return;
    };
    let log = run(&godot, "prf_12_walkthrough_test.gd");
    assert!(log.contains("prf_12_walkthrough_runtime_and_exit_summary: PASS"));
    assert!(log.contains("WALKTHROUGH stopped: ready=true"));
    let summary = log
        .lines()
        .find(|line| line.starts_with("SOE_FRAME_REPORT "))
        .expect("walkthrough shutdown writes a frame summary");
    for system in soe_trace::System::ALL {
        assert!(summary.contains(system.name()), "missing {}", system.name());
    }
}
