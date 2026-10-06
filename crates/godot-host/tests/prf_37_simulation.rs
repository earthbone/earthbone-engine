// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-37 host timing contracts and installed TST-18 rendered frames.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

fn run(script: &str, windowed: bool) -> String {
    let binary = std::env::var_os("SOE_GODOT_BIN").expect("host checks require SOE_GODOT_BIN");
    let root = workspace();
    assert!(
        root.join("target/release/libsoe_godot_host.so").is_file(),
        "build the release extension and import godot before these checks"
    );
    let mut command = Command::new(binary);
    command.args(["--path"]).arg(root.join("godot"));
    if windowed {
        command.args(["--resolution", "640x360"]);
    } else {
        command.arg("--headless");
    }
    command.args(["--script", script]);
    if windowed {
        command
            .env("SOE_THREADED_SIMULATION", "1")
            .env("SOE_PRF_37_REPORT", root.join("target/prf-37-timing.json"))
            .env_remove("SOE_CAPTURE_PATH")
            .env_remove("SOE_AUD_11_TEST");
    }
    let output = command.output().unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let name = if windowed { "windowed" } else { "gate" };
    std::fs::write(root.join(format!("target/prf-37-{name}.log")), &log).unwrap();
    println!("{log}");
    assert!(
        output.status.success(),
        "{script} exit {:?}, see target/prf-37-{name}.log",
        output.status.code()
    );
    assert!(!log.contains("SCRIPT ERROR:"), "{log}");
    log
}

#[test]
fn prf_37_tst_18_rejects_main_thread_percentiles_and_simulation_lag() {
    if std::env::var_os("SOE_GODOT_BIN").is_none() {
        return;
    }
    assert!(run("prf_37_timing_gate_test.gd", false)
        .contains("prf_37_tst_18_rejects_main_thread_percentiles_and_simulation_lag: PASS"));
}

#[test]
fn prf_37_installed_windowed_tst_18_gate() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let log = run("tst_18_frame_budget_test.gd", true);
    assert!(log.contains("TST-18 bannered_mare") && log.contains("TST-18 whiterun_plains"));
    assert!(log.contains("prf_37_installed_windowed_tst_18_gate: PASS"));
    assert!(log.contains("WALKTHROUGH stopped"));
    let summary = log
        .lines()
        .find(|line| line.starts_with("PRF-37 simulation:"))
        .expect("shutdown reports the last simulation sample");
    assert!(summary.contains("\"ready\":true"), "{summary}");
}
