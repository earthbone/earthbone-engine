// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Cold package planning across overlapping NAVM floors.

use std::process::Command;

#[test]
fn tst_18_cold_actor_packages_bound_lookup_on_overlapping_navmesh_floors() {
    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .args([
            "stress",
            "actors",
            "--load",
            "8",
            "--samples",
            "120",
            "--warmup",
            "0",
        ])
        .output()
        .expect("cold actor stress starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");

    let sample = stdout
        .lines()
        .find_map(|line| line.strip_prefix("SOE_SAMPLE "))
        .expect("stress report contains a sample");
    let report: serde_json::Value = serde_json::from_str(sample).expect("sample is JSON");
    assert_eq!(report["actors"].as_u64(), Some(8));
    assert_eq!(report["navmesh_triangles"].as_u64(), Some(60_002));
    assert!(
        report["package_evaluations"].as_u64().expect("evaluations") >= 8,
        "{sample}"
    );

    for operation in ["navmesh_locate", "navmesh_triangles"] {
        let prefix = format!("work_peak_{operation} ");
        let count = stdout
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .and_then(|line| line.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| panic!("missing work counter {operation}: {stdout}"));
        assert!(count > 0, "{operation} did not run: {stdout}");
        if operation == "navmesh_triangles" {
            assert!(count <= 4_000, "one tick examined {count} triangles");
        }
    }
}
