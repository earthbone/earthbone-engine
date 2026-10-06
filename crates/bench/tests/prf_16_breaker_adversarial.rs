// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

#[test]
fn prf_16_thousand_actor_run_reports_full_navmesh_workload() {
    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .arg("frame-budget")
        .output()
        .expect("run fixed thousand-actor workload");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report = stdout
        .lines()
        .find_map(|line| line.strip_prefix("SOE_BUDGET "))
        .expect("one fixed workload report");
    let report: serde_json::Value = serde_json::from_str(report).expect("valid JSON report");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(report["scenario"], "thousand-actors");
    assert_eq!(report["actors"], 1000);
    assert_eq!(report["samples"], 3600);
    assert_eq!(report["game_ticks"], 3600);
    assert_eq!(report["simulated_s"], 60);
    assert_eq!(report["navmesh"], true);
    assert!(report["evaluations"].as_u64().unwrap() >= 1000);
    assert_eq!(
        report["systems"].as_object().unwrap().len(),
        soe_trace::SYSTEM_COUNT
    );
}
