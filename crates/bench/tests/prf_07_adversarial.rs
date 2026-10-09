// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-07 runner and TST-18 frame limits from docs/performance.md.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn optimized_binary() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Cargo names itself in CARGO for the tests it runs. Using that path keeps
    // the nested build on the toolchain of the outer run and out of any PATH
    // wrapper that serializes builds, which a test holding a slot would wait on.
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let build = Command::new(cargo)
        .current_dir(root)
        .args([
            "build",
            "-p",
            "soe-bench",
            "--release",
            "--bin",
            "soe-bench",
            "--message-format=json",
        ])
        .output()
        .expect("build optimized stress child");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    String::from_utf8_lossy(&build.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|row| {
            (row["reason"] == "compiler-artifact" && row["target"]["name"] == "soe-bench")
                .then(|| row["executable"].as_str().map(PathBuf::from))
                .flatten()
        })
        .expect("Cargo reports the optimized executable path")
}

fn cold_sample(binary: &Path) -> serde_json::Value {
    let logs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../performance-results/tst-18");
    std::fs::create_dir_all(&logs).expect("create stress log directory");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let log = logs.join(format!("cold-stress-{timestamp}.log"));
    let output = std::fs::File::create(&log).expect("create cold stress log");
    let mut child = Command::new(binary)
        .args([
            "stress",
            "actors",
            "--load",
            "1000",
            "--samples",
            "120",
            "--warmup",
            "0",
        ])
        .stdout(Stdio::from(output.try_clone().expect("clone log handle")))
        .stderr(Stdio::from(output))
        .spawn()
        .expect("launch optimized cold stress child");
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("read stress child status") {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait().expect("reap stress child");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let output = std::fs::read_to_string(log).expect("read cold stress output");
    assert!(
        !timed_out && status.success(),
        "status={status} timed_out={timed_out}\n{output}"
    );
    let rows: Vec<_> = output
        .lines()
        .filter_map(|line| line.strip_prefix("SOE_SAMPLE "))
        .collect();
    assert_eq!(rows.len(), 1, "{output}");
    serde_json::from_str(rows[0]).expect("valid cold stress sample")
}

fn check_sample(sample: &serde_json::Value) {
    assert_eq!(sample["samples"], 120);
    assert_eq!(sample["actors"], 1000);
    assert_eq!(sample["navmesh_triangles"], 60_002);
    assert!(
        sample["package_evaluations"]
            .as_u64()
            .expect("package evaluations")
            >= 1_000
    );
    let p99 = sample["p99_ms"].as_f64().expect("numeric p99");
    let worst = sample["worst_ms"].as_f64().expect("numeric worst frame");
    eprintln!("TST-18 actors=1000 navmesh_triangles=60002 p99_ms={p99:.3} worst_ms={worst:.3}");
    assert!(
        p99.is_finite() && p99 > 0.0 && p99 <= 16.7,
        "p99 {p99} > 16.7 ms"
    );
    assert!(
        worst.is_finite() && worst >= p99 && worst <= 33.0,
        "worst {worst} > 33 ms"
    );
    let rss = sample["rss_mib"].as_f64().expect("Linux process RSS");
    assert!(rss.is_finite() && rss < 2_048.0, "RSS {rss} >= 2,048 MiB");
}

#[test]
fn tst_18_prf_07_runner_asserts_thousand_actor_p99_and_worst_limits() {
    let binary = optimized_binary();
    check_sample(&cold_sample(&binary));
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../performance-results/tst-18-runner-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&out);
    let runner = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/performance.py");
    let result = Command::new("python3")
        .arg(runner)
        .args([
            "check",
            "--scenario",
            "actors",
            "--binary",
            binary.to_str().expect("benchmark executable path"),
            "--start",
            "1000",
            "--samples",
            "120",
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
        "runner failed: {}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let report = std::fs::read_to_string(out.join("report.json")).expect("read run report");
    assert!(report.contains("\"status\": \"pass\""), "{report}");
    assert!(report.contains("\"actors\": 1000"), "{report}");
    assert!(report.contains("\"peak_allocations\""), "{report}");
    assert!(report.contains("\"peak_allocation_bytes\""), "{report}");
    assert!(report.contains("\"p99_ms\""), "{report}");
    assert!(report.contains("\"worst_ms\""), "{report}");
    assert!(report.contains("\"rss_mib\""), "{report}");
    let report: serde_json::Value = serde_json::from_str(&report).expect("valid run report");
    let samples = report["runs"][0]["samples"]
        .as_array()
        .expect("measured windows");
    assert_eq!(samples.len(), 1);
    check_sample(&samples[0]);
    let _ = std::fs::remove_dir_all(out);
}
