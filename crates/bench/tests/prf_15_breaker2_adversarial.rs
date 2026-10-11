// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const OPERATIONS: [(&str, u64); 9] = [
    ("form_lookup", 1024),
    ("winning_override_resolve", 1024),
    ("navmesh_locate", 1024),
    ("navmesh_find_path", 128),
    ("papyrus_call", 1),
    ("papyrus_native_call", 1),
    ("nif_parse", 128),
    ("dds_bc7_decode", 1),
    ("bsa_file_read", 128),
];
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "soe-prf15-breaker2-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn divan_output() -> String {
    let mut text = String::new();
    for (name, batch) in OPERATIONS {
        writeln!(text, "├─ {name} 1 ns │ 1 ns │ {} ns │ {} ns │ 30 │ 100", 100 * batch, 100 * batch).unwrap();
    }
    text
}

fn run(mode: &str, output: &Path, baseline: &Path) -> Output {
    Command::new("python3")
        .arg(root().join("tools/perf_baselines.py"))
        .arg(mode)
        .arg("--output")
        .arg(output)
        .arg("--baseline")
        .arg(baseline)
        .env_remove("SKYRIM_DATA")
        .output()
        .expect("baseline helper runs")
}

#[test]
fn prf_15_malformed_operations_array_returns_a_diagnostic() {
    let measured = scratch("malformed-output");
    let baseline = scratch("malformed-baseline");
    fs::write(&measured, divan_output()).unwrap();
    assert!(run("refresh", &measured, &baseline).status.success());

    let mut saved: serde_json::Value = serde_json::from_slice(&fs::read(&baseline).unwrap()).unwrap();
    saved["operations"] = serde_json::json!(["form_lookup"]);
    fs::write(&baseline, serde_json::to_vec(&saved).unwrap()).unwrap();

    let result = run("check", &measured, &baseline);
    assert_eq!(result.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!stderr.contains("Traceback"), "unexpected traceback: {stderr}");

    let _ = fs::remove_file(measured);
    let _ = fs::remove_file(baseline);
}

#[test]
fn prf_15_baseline_from_another_host_is_rejected() {
    let measured = scratch("host-output");
    let baseline = scratch("host-baseline");
    fs::write(&measured, divan_output()).unwrap();
    assert!(run("refresh", &measured, &baseline).status.success());

    let mut saved: serde_json::Value = serde_json::from_slice(&fs::read(&baseline).unwrap()).unwrap();
    saved["machine"]["host"] = serde_json::Value::String("other-machine".into());
    fs::write(&baseline, serde_json::to_vec(&saved).unwrap()).unwrap();

    let result = run("check", &measured, &baseline);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("baseline machine does not match"));

    let _ = fs::remove_file(measured);
    let _ = fs::remove_file(baseline);
}
