// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "requires an optimized build and an idle machine, see docs/performance-limits.md"]
fn prf_07_runner_records_thousand_actor_budget_sample() {
    let out = std::env::temp_dir().join(format!("soe-prf-07-adversarial-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let runner = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/performance.py");
    let result = Command::new("python3")
        .arg(runner)
        .args([
            "check",
            "--scenario",
            "actors",
            "--binary",
            env!("CARGO_BIN_EXE_soe-bench"),
            "--start",
            "1000",
            "--samples",
            "3",
            "--warmup",
            "1",
            "--repeats",
            "1",
            "--out",
        ])
        .arg(&out)
        .output()
        .expect("run thousand actor gate");

    assert!(
        result.status.success(),
        "runner failed: {}",
        String::from_utf8_lossy(&result.stdout)
    );
    let report = std::fs::read_to_string(out.join("report.json")).expect("read run report");
    assert!(report.contains("\"status\": \"pass\""), "{report}");
    assert!(report.contains("\"actors\": 1000"), "{report}");
    assert!(report.contains("\"peak_allocations\""), "{report}");
    assert!(report.contains("\"peak_allocation_bytes\""), "{report}");
    assert!(report.contains("\"p99_ms\""), "{report}");
    assert!(report.contains("\"worst_ms\""), "{report}");
    assert!(report.contains("\"rss_mib\""), "{report}");
    let _ = std::fs::remove_dir_all(out);
}
