// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed windowed PRF-36 and retained PRF-11 checks against the release extension.

use std::path::Path;
use std::process::Command;

#[test]
fn prf_36_installed_windowed_plains_budget_and_bannered_mare_regression() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("installed check requires SOE_GODOT_BIN");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut failures = Vec::new();
    for script in ["prf_36_ai_budget_test.gd", "prf_11_ai_planning_check.gd"] {
        let log_path = root.join("target").join(format!("{script}.log"));
        let log_file = std::fs::File::create(&log_path).unwrap();
        let status = Command::new(&godot)
            .current_dir(&root)
            .args([
                "--path",
                "godot",
                "--resolution",
                "640x360",
                "--script",
                script,
            ])
            .env_remove("SOE_PRF_36_BASELINE")
            .env("SOE_GODOT_TIMEOUT", "1200")
            .stdout(log_file.try_clone().unwrap())
            .stderr(log_file)
            .status()
            .unwrap();
        let log = std::fs::read_to_string(log_path).unwrap();
        if !status.success() || log.contains("SCRIPT ERROR") {
            failures.push(format!("{script}: {log}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
