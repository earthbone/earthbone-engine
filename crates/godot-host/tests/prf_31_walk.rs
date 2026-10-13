// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "godot")]

//! PRF-31 launches the collision-guided walk checks against the prepared host.

use std::path::Path;
use std::process::Command;

#[test]
fn prf_31_installed_one_sided_obstructions_keep_directional_sight() {
    let (Some(godot), Some(data)) = (
        std::env::var_os("SOE_GODOT_BIN"),
        std::env::var_os("SKYRIM_DATA"),
    ) else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .arg("--headless")
        .arg("--path")
        .arg(root.join("godot"))
        .args(["--script", "prf_31_detection_test.gd"])
        .env("SKYRIM_DATA", data)
        .env("PRF_31_BASELINE", "1")
        .env("SOE_VERIFY_DETECTION_OCCLUSION", "1")
        .env("SOE_TEST_ONE_SIDED_DETECTION", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("mismatches=0"), "{stdout}");
    assert!(stdout.contains("PRF31 reciprocal checked=0"), "{stdout}");
    assert!(stdout.contains("RESULT: PASS PRF31 detection"), "{stdout}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}

#[test]
fn prf_31_installed_actor_occlusion_matches_godot_rays() {
    let (Some(godot), Some(data)) = (
        std::env::var_os("SOE_GODOT_BIN"),
        std::env::var_os("SKYRIM_DATA"),
    ) else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .arg("--headless")
        .arg("--path")
        .arg(root.join("godot"))
        .args(["--script", "prf_31_detection_test.gd"])
        .env("SKYRIM_DATA", data)
        .env("PRF_31_BASELINE", "1")
        .env("SOE_VERIFY_DETECTION_OCCLUSION", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let verified = stdout
        .lines()
        .find_map(|line| line.strip_prefix("PRF31 occlusion checked="))
        .unwrap();
    let (count, result) = verified.split_once(' ').unwrap();
    assert!(count.parse::<usize>().unwrap() >= 100, "{stdout}");
    assert_eq!(result, "mismatches=0", "{stdout}");
    let reciprocal = stdout
        .lines()
        .find_map(|line| line.strip_prefix("PRF31 reciprocal checked="))
        .unwrap();
    assert!(reciprocal.parse::<usize>().unwrap() > 0, "{stdout}");
    assert!(stdout.contains("RESULT: PASS PRF31 detection"), "{stdout}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
    println!("PRF31 occlusion checked={verified}");
    println!("PRF31 reciprocal checked={reciprocal}");
}

#[test]
fn prf_31_walk_route_turns_on_stalls_without_moving_the_player() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = Command::new(godot)
        .arg("--headless")
        .arg("--path")
        .arg(project)
        .args(["--script", "prf_31_walk_route_test.gd"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("RESULT: PASS PRF31 walk route"), "{stdout}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}

#[test]
fn prf_31_installed_windowed_gate_walk_keeps_every_detection_update_under_two_ms() {
    let (Some(godot), Some(data)) = (
        std::env::var_os("SOE_GODOT_BIN"),
        std::env::var_os("SKYRIM_DATA"),
    ) else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(
        root.join("target/release/libsoe_godot_host.so").is_file(),
        "build the release extension and import the Godot project first"
    );
    let output = Command::new(godot)
        .arg("--path")
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "prf_31_detection_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env_remove("PRF_31_BASELINE")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("PRF31 walk seconds=60"), "{stdout}");
    assert!(stdout.contains("over_budget=0"), "{stdout}");
    assert!(stdout.contains("RESULT: PASS PRF31 detection"), "{stdout}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
    for line in stdout.lines().filter(|line| {
        line.starts_with("PRF31 exterior cell=")
            || line.starts_with("PRF31 walk seconds=")
            || (line.starts_with("PRF31 phase=") && line.contains(" seconds="))
    }) {
        println!("{line}");
    }
}
