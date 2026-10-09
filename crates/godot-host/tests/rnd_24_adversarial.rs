// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Run the installed Bannered Mare glow-map capture through the Godot renderer.

use std::{path::PathBuf, process::Command};

#[test]
fn rnd_24_installed_chandelier_emission_is_limited_to_glow_map_pixels() {
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
            "rnd_24_emission_capture_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot capture process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("rnd_24_installed_bannered_mare_chandelier_glow_is_masked"),
        "Godot chandelier capture failed\n{stdout}\n{stderr}"
    );

    let line = stdout
        .lines()
        .find(|line| line.contains("rnd_24_installed_bannered_mare_chandelier_glow_is_masked"))
        .expect("capture reports pixel counts");
    let count = |name: &str| -> usize {
        line.split_whitespace()
            .find_map(|field| field.strip_prefix(&format!("{name}=")))
            .expect("capture pixel count exists")
            .parse()
            .expect("capture pixel count is numeric")
    };
    let visible = count("visible");
    let bright = count("bright");
    assert!(visible > 20, "chandelier has {visible} visible pixels");
    assert!(bright > 0, "glow map produces no bright candle pixels");
    assert!(
        bright * 2 < visible,
        "emission covers {bright} of {visible} visible pixels"
    );
    assert!(
        workspace
            .join("target/rnd_24_bannered_mare_chandelier.png")
            .is_file(),
        "installed chandelier capture image is missing"
    );
}
