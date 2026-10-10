// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed Whiterun city and parent world LOD capture.

use std::{path::PathBuf, process::Command};

#[test]
fn rnd_27_capture_shows_city_cells_parent_plains_and_city_silhouette() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN");
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let extension = project.join("../target/release/libsoe_godot_host.so");
    assert!(
        extension.is_file(),
        "build the release extension before capture"
    );
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_27_whiterun_capture.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot capture starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RND-27 RESULT: PASS"),
        "Whiterun capture failed: {stdout}\n{stderr}"
    );
    for image in [
        "dragonsreach-plains.png",
        "outside-gate-city.png",
        "outside-gate-no-city.png",
    ] {
        assert!(
            project.join(".godot/rnd_27").join(image).is_file(),
            "missing {image}"
        );
    }
}
