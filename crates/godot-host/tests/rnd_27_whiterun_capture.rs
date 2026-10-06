// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! RND-27 windowed installed-data city and parent-world LOD acceptance check.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_27_installed_dragonsreach_plains_and_outside_gate_city_capture() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("Godot for installed RND-27 capture");
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
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
        .expect("start RND-27 capture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = project.join(".godot/rnd_27/capture.log");
    std::fs::create_dir_all(log.parent().expect("capture directory"))
        .expect("create capture directory");
    std::fs::write(&log, format!("{stdout}\n{stderr}")).expect("write capture log");
    let diagnostics = stdout
        .lines()
        .chain(stderr.lines())
        .filter(|line| line.starts_with("RND-27") || line.starts_with("SCRIPT ERROR:"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output.status.success() && stdout.contains("RND-27 RESULT: PASS"),
        "RND-27 capture failed, log at {}\n{diagnostics}",
        log.display(),
    );
    for name in [
        "dragonsreach-plains.png",
        "dragonsreach-no-terrain-lod.png",
        "outside-gate-city.png",
        "outside-gate-no-city.png",
    ] {
        assert!(
            project.join(".godot/rnd_27").join(name).is_file(),
            "missing {name}"
        );
    }
    for line in stdout.lines().filter(|line| line.starts_with("RND-27")) {
        println!("{line}");
    }
}
