// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn prf_16_runner_checks_every_budget_and_preserves_sweep_failures() {
    let result = Command::new("python3")
        .args([
            "-m",
            "unittest",
            "discover",
            "-s",
            "tools/tests",
            "-p",
            "test_frame_budget.py",
        ])
        .current_dir(root())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "fixed three-minute installed CPU suite requires release binaries and an idle machine"]
fn prf_16_installed_sweep_runs_fixed_scenarios_and_records_budget_breaches() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKIP PRF-16: SKYRIM_DATA is unset");
        return;
    };
    let output = root().join("target/prf-16-installed-test");
    let result = Command::new("bash")
        .arg(root().join("tools/perf-budget.sh"))
        .args([
            "--no-build",
            "--sweep",
            "--binary",
            env!("CARGO_BIN_EXE_soe-bench"),
            "--out",
        ])
        .arg(&output)
        .env("SKYRIM_DATA", data)
        .current_dir(root())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(output.join("report.json")).unwrap())
            .unwrap();
    let results = report["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    let mut failed_scenarios = 0;
    for result in results {
        assert_ne!(result["status"], "error", "{result}");
        let sample = &result["sample"];
        assert!(sample["samples"].as_u64().unwrap() >= 3600);
        assert_eq!(sample["game_ticks"], sample["samples"]);
        assert!(sample["elapsed_s"].as_f64().unwrap() >= 60.0);
        assert!(sample["simulated_s"].as_f64().unwrap() >= 60.0);
        assert_eq!(sample["navmesh"], true);
        assert!(sample["evaluations"].as_u64().unwrap() > 0);
        if result["scenario"] == "thousand-actors" {
            assert_eq!(sample["actors"], 1000);
        }
        if result["scenario"] == "walk-out" {
            assert_eq!(sample["walked"], true);
            assert!(sample["exterior_actors"].as_u64().unwrap() >= 5);
            assert_eq!(sample["world_editor_id"], "WhiterunWorld");
        }
        let systems = sample["systems"].as_object().unwrap();
        assert_eq!(systems.len(), soe_trace::SYSTEM_COUNT);
        let mut breached = false;
        for name in
            std::iter::once("frame").chain(soe_trace::System::ALL.map(soe_trace::System::name))
        {
            let stats = if name == "frame" {
                &sample["frame"]
            } else {
                &systems[name]
            };
            for metric in ["p50_ms", "p99_ms", "worst_ms"] {
                let value = stats[metric].as_f64().unwrap();
                assert!(value.is_finite() && value >= 0.0);
                breached |= value > report["budgets"][name][metric].as_f64().unwrap();
            }
        }
        assert_eq!(result["status"], if breached { "breach" } else { "pass" });
        failed_scenarios += usize::from(breached);
    }
    assert_eq!(
        report["followup_rows"].as_array().unwrap().len(),
        failed_scenarios
    );
}
