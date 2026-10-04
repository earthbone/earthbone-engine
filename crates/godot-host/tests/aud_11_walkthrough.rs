// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Runs the installed-data walkthrough in a window and checks its audio output.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn aud_11_walkthrough_audio_runs_for_twenty_seconds_and_reports_each_playing_system() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return;
    };

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));
    let extension = target.join("release/libsoe_godot_host.so");
    assert!(
        extension.exists(),
        "build the release extension before the windowed walkthrough: {}",
        extension.display()
    );

    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--windowed", "--resolution", "640x360", "--script"])
        .arg(workspace.join("godot/walkthrough.gd"))
        .env("SKYRIM_DATA", data)
        .env("SOE_AUD_11_TEST", "1")
        .output()
        .expect("Godot walkthrough process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("AUD-11 RESULT: PASS"),
        "windowed AUD-11 walkthrough failed with {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    for system in [
        "footsteps=",
        "ambience=",
        "music=",
        "voices=",
        "master_peak_db=",
    ] {
        assert!(
            stdout.contains(system),
            "missing {system} in walkthrough report\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
    assert!(
        stdout.contains("walk_seconds=20"),
        "walk duration was not reported\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
