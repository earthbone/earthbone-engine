// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn prf_16_host_timing_window_rejects_nesting_and_unmatched_end() {
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = Command::new(godot)
        .args([
            "--headless",
            "--path",
            "godot",
            "--script",
            "prf_16_metrics_test.gd",
        ])
        .env("XDG_DATA_HOME", root.join("target/prf-16-adversarial-user"))
        .current_dir(&root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);

    assert!(result.status.success(), "{stdout}\n{stderr}");
    assert!(!stderr.contains("ERROR:"), "{stderr}");
    assert!(
        stdout.contains("prf_16_timing_windows_and_full_run_percentiles: PASS"),
        "{stdout}"
    );
}
