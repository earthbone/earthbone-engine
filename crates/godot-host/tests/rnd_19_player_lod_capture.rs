// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Runs the RND-19 gameplay-host exterior LOD capture on installed Skyrim data.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_19_player_exterior_lod_tracks_gameplay_and_captures_distant_terrain() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping the installed RND-19 capture check");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("SOE_GODOT_BIN not set, skipping the Godot RND-19 capture check");
        return;
    };
    assert!(
        extension.exists(),
        "release extension missing at {}, build soe-godot-host first",
        extension.display()
    );

    let capture = workspace.join("docs/images/rnd-19-whiterun-exterior-lod.png");
    let script = project.join("rnd_19_player_lod_capture.gd");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--resolution", "640x360", "--script"])
        .arg(script)
        .env("SKYRIM_DATA", data)
        .env("SOE_RND19_CAPTURE_PATH", &capture)
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("RESULT: PASS: PlayerNode streams terrain LOD around Whiterun"),
        "Godot RND-19 capture failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        capture.is_file(),
        "Godot did not save {}",
        capture.display()
    );
    println!("{stdout}");
}
