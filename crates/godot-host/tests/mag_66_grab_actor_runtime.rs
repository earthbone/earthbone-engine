// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn mag_66_runtime_holds_throws_and_releases_actor_on_expiry() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: set SKYRIM_DATA to the installed Skyrim Data folder");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: set SOE_GODOT_BIN to Godot 4.4 or newer");
        return;
    };

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
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
        "MAG-66 Godot runtime test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
