// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::process::Command;

#[test]
#[ignore = "requires an optimized build and an idle machine, see docs/performance-limits.md"]
fn tst_18_thousand_navmesh_actors_meet_frame_budgets() {
    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .args([
            "stress",
            "actors",
            "--load",
            "1000",
            "--samples",
            "120",
            "--warmup",
            "10",
        ])
        .output()
        .expect("actor stress benchmark starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");

    let sample = stdout
        .lines()
        .find_map(|line| line.strip_prefix("SOE_SAMPLE "))
        .expect("stress report contains a sample");
    let report: serde_json::Value = serde_json::from_str(sample).expect("sample is JSON");
    assert_eq!(report["actors"].as_u64(), Some(1000));
    assert!(report["p99_ms"].as_f64().expect("p99") <= 16.7, "{sample}");
    assert!(
        report["worst_ms"].as_f64().expect("worst") <= 33.0,
        "{sample}"
    );
    for operation in ["navmesh_locate", "navmesh_triangles", "path_nodes"] {
        let prefix = format!("work_peak_{operation} ");
        let count = stdout
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .and_then(|line| line.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| panic!("missing work counter {operation}: {stdout}"));
        assert!(count > 0, "{operation} did not run: {stdout}");
    }
}
