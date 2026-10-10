// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-24 runs the production walkthrough loop and Godot shutdown lifecycle.
//! Input injection and window close follow the local Godot Input and SceneTree docs.

use std::path::PathBuf;
use std::process::Command;

fn run(mode: &str, installed: bool) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = std::env::var_os("SOE_GODOT_BIN").expect("HST-24 needs SOE_GODOT_BIN");
    assert!(
        root.join("target/release/libsoe_godot_host.so").is_file(),
        "build the release extension and import godot before HST-24"
    );
    let output = Command::new(binary)
        .arg("--headless")
        .arg("--path")
        .arg(root.join("godot"))
        .args(["--script", "hst_24_pause_test.gd"])
        .env("SOE_HST_24_MODE", mode)
        .env("SOE_HST_24_INSTALLED", if installed { "1" } else { "0" })
        .env_remove("SOE_CAPTURE_PATH")
        .env_remove("SOE_AUD_11_TEST")
        .output()
        .expect("run HST-24 host check");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(
        root.join(format!("target/hst_24_{mode}_{installed}.log")),
        &log,
    )
    .unwrap();
    assert!(output.status.success(), "HST-24 {mode} failed: {log}");
    assert!(!log.contains("SCRIPT ERROR:"), "{log}");
    assert!(!log.contains("HST-24 FAIL:"), "{log}");
    assert!(log.contains("WALKTHROUGH stopped"), "{log}");
    if mode != "metrics" {
        assert!(log.contains("hst_24_escape_toggles_pause: PASS"), "{log}");
        if installed {
            assert!(log.contains("STARTUP ready ms="), "{log}");
        }
    }
    println!("{log}");
    log
}

#[test]
fn hst_24_escape_toggles_pause_and_quit_entry_stops_walkthrough() {
    if std::env::var_os("SOE_GODOT_BIN").is_none() {
        return;
    }
    assert!(run("quit", false).contains("hst_24_pause_quit_entry: PASS"));
}

#[test]
fn hst_24_window_close_stops_walkthrough() {
    if std::env::var_os("SOE_GODOT_BIN").is_none() {
        return;
    }
    assert!(run("window", false).contains("hst_24_window_close: PASS"));
}

#[test]
fn hst_24_overlay_metrics_without_viewport() {
    if std::env::var_os("SOE_GODOT_BIN").is_none() {
        return;
    }
    assert!(run("metrics", false).contains("hst_24_overlay_metrics_without_viewport: PASS"));
}

#[test]
fn hst_24_installed_walkthrough_pause_and_shutdown() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    assert!(run("quit", true).contains("hst_24_pause_quit_entry: PASS"));
    assert!(run("window", true).contains("hst_24_window_close: PASS"));
}
