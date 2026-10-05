// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed RND-22 captures use the production material cache and Godot renderer.

use std::{path::PathBuf, process::Command};

#[test]
fn rnd_22_installed_mist_and_window_captures_are_translucent_and_bounded() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
    assert!(
        workspace
            .join("target/release/libsoe_godot_host.so")
            .is_file(),
        "build the release extension and import the Godot project before this check"
    );
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_22_effect_material_test.gd",
            "--",
            "--vanilla-only",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot capture process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("rnd_22_effect_material_capture: PASS"),
        "Godot effect capture failed\n{stdout}\n{stderr}"
    );
    for name in ["mist", "window"] {
        assert!(stdout.contains(&format!("rnd_22_vanilla {name} pixels=")));
        assert!(project.join(format!(".godot/rnd_22-{name}.png")).is_file());
    }
    println!("{stdout}");
}
