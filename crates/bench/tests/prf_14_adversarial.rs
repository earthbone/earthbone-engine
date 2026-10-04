// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::process::Command;

fn run_stress(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .arg("stress")
        .args(args)
        .output()
        .expect("stress benchmark starts")
}

fn assert_work(output: &std::process::Output, expected: &[(&str, u64, u64)]) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    for (operation, low, high) in expected {
        let prefix = format!("work_peak_{operation} ");
        let line = stdout
            .lines()
            .find(|line| line.starts_with(&prefix))
            .unwrap_or_else(|| panic!("missing {prefix} in {stdout}"));
        let value: u64 = line[prefix.len()..]
            .split_whitespace()
            .next()
            .expect("counter value")
            .parse()
            .expect("counter is an integer");
        assert!((*low..=*high).contains(&value), "{line}");
    }
}

#[test]
fn prf_14_thousand_actor_stress_scenario_stays_within_fixed_work_bounds() {
    let output = run_stress(&[
        "actors",
        "--load",
        "1000",
        "--samples",
        "120",
        "--warmup",
        "10",
    ]);
    assert_work(
        &output,
        &[
            ("record_lookups", 1, 8_000),
            ("navmesh_locate", 1, 16_000),
            ("navmesh_triangles", 0, 4_000),
            ("path_nodes", 0, 200_000),
        ],
    );
}

#[test]
fn prf_14_mixed_stress_scenario_counts_scripts_and_path_search() {
    let output = run_stress(&["mixed", "--load", "16", "--samples", "5", "--warmup", "0"]);
    assert_work(
        &output,
        &[
            ("papyrus_instructions", 544, 544),
            ("native_calls", 0, 256),
            ("path_nodes", 2_048, 10_000),
            ("record_lookups", 0, 8_192),
        ],
    );
}
