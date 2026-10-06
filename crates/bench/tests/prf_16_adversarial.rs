// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

#[test]
fn prf_16_fixed_runner_rejects_workload_overrides_before_sampling() {
    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .args(["frame-budget", "--samples", "1"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("frame-budget takes no options"), "{stderr}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("SOE_BUDGET"));
}
