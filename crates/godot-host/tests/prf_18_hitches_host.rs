// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

//! Runs `godot/prf_18_hitches_test.gd` against the built extension in an isolated
//! project, so its `user://hitches` directory is not shared with other Godot runs.
//! Gated on SKYRIM_DATA with the other installed host checks, though the script
//! itself reads no game records.

#[path = "common/isolated_project.rs"]
mod isolated_project;

#[test]
fn prf_18_host_script_dumps_prior_frames_into_user_hitches() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let library = workspace.join("target/release/libsoe_godot_host.so");
    let project = isolated_project::project(&["prf_18_hitches_test.gd"], Some(&library));
    let output = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project.path())
        .args(["--script", "prf_18_hitches_test.gd"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("prf_18_host_slow_frame_keeps_five_seconds_and_writes_user_hitches: PASS"),
        "{stdout}\n{stderr}"
    );
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}
