// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed PRF-37 gate checks through the Godot host.

use std::path::Path;
use std::process::Command;

fn run(script: &str, installed: bool) -> String {
    let binary = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN required");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let mut command = Command::new(binary);
    command.args(["--path"]).arg(root.join("godot")).arg("--headless").args(["--script", script]);
    if installed {
        command.env("SOE_THREADED_SIMULATION", "1");
    }
    let output = command.output().unwrap();
    let log = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "Godot {script} failed: {log}");
    assert!(!log.contains("SCRIPT ERROR:"), "{log}");
    log
}

#[test]
fn prf_37_adversarial_synthetic_gate_rejects_bad_timing_samples() {
    if std::env::var_os("SOE_GODOT_BIN").is_none() { return; }
    let log = run("prf_37_timing_gate_test.gd", false);
    assert!(log.contains("prf_37_tst_18_rejects_main_thread_percentiles_and_simulation_lag: PASS"), "{log}");
}

#[test]
fn prf_37_adversarial_installed_bannered_mare_and_plains_gate() {
    if std::env::var_os("SKYRIM_DATA").is_none() || std::env::var_os("SOE_GODOT_BIN").is_none() { return; }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    assert!(root.join("target/release/libsoe_godot_host.so").is_file(), "build release extension before installed check");
    let mut command = Command::new(std::env::var_os("SOE_GODOT_BIN").unwrap());
    command.args(["--path"]).arg(root.join("godot")).args(["--resolution", "640x360", "--script", "tst_18_frame_budget_test.gd"]);
    command.env("SOE_THREADED_SIMULATION", "1");
    command.env("SOE_PRF_37_REPORT", root.join("target/prf-37-adversarial.json"));
    let output = command.output().unwrap();
    let log = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "installed TST-18 failed: {log}");
    assert!(log.contains("TST-18 bannered_mare") && log.contains("TST-18 whiterun_plains"), "{log}");
    assert!(log.contains("prf_37_installed_windowed_tst_18_gate: PASS"), "{log}");
    assert!(log.contains("WALKTHROUGH stopped"), "{log}");
}
