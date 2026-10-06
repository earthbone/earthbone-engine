// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

fn output_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("soe-{name}-{}", std::process::id()))
}

#[test]
fn prf_06_allocation_regression_fails_the_performance_gate() {
    let out = output_dir("prf-06-adversarial");
    let _ = std::fs::remove_dir_all(&out);
    let runner = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/performance.py");
    let result = Command::new("python3")
        .arg(runner)
        .args([
            "check",
            "--scenario",
            "scripts",
            "--binary",
            env!("CARGO_BIN_EXE_soe-bench"),
            "--start",
            "1",
            "--samples",
            "1",
            "--warmup",
            "1",
            "--max-allocations",
            "1",
            "--out",
        ])
        .arg(&out)
        .output()
        .expect("run performance gate");

    assert_eq!(result.status.code(), Some(2));
    let report = std::fs::read_to_string(out.join("report.json")).unwrap_or_else(|error| {
        panic!(
            "read gate report: {error}; stdout={}; stderr={}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        )
    });
    assert!(
        report.contains("\"status\": \"breach\""),
        "allocation breach did not fail the runner gate: {report}"
    );
    assert!(
        report.contains("peak_allocations=") && report.contains("exceeds 1"),
        "allocation threshold was not the breach reason: {report}"
    );
    let _ = std::fs::remove_dir_all(out);
}

#[test]
fn prf_06_each_cpu_scenario_reports_allocation_counts() {
    for scenario in ["scripts", "actors", "navigation", "streaming", "mixed"] {
        let result = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
            .args([
                "stress",
                scenario,
                "--load",
                "1",
                "--samples",
                "1",
                "--warmup",
                "1",
            ])
            .output()
            .expect("run CPU scenario");

        assert!(
            result.status.success(),
            "{scenario} failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let output = String::from_utf8_lossy(&result.stdout);
        let sample = output
            .lines()
            .find(|line| line.starts_with("SOE_SAMPLE "))
            .unwrap_or_else(|| panic!("{scenario} did not emit a sample: {output}"));
        assert!(
            sample.contains("\"peak_allocations\":"),
            "{scenario} omitted peak allocation count: {sample}"
        );
        assert!(
            sample.contains("\"peak_allocation_bytes\":"),
            "{scenario} omitted peak allocation bytes: {sample}"
        );
    }
}
