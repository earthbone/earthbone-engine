// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn mag_66_grab_actor_holds_and_throws_after_effect_interruption() {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from);
    let godot = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from);
    let (Some(data), Some(godot)) = (data, godot) else {
        panic!("MAG-66 runtime check requires SKYRIM_DATA and SOE_GODOT_BIN");
    };

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = manifest.join("../../godot");
    let script = project.join("mag_66_grab_actor_runtime_test.gd");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(project)
        .args(["--resolution", "640x360", "--script"])
        .arg(script)
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot runtime process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS MAG-66"),
        "MAG-66 runtime check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
