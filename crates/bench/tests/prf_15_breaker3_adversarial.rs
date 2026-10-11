// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const OPERATIONS: [&str; 9] = [
    "form_lookup",
    "winning_override_resolve",
    "navmesh_locate",
    "navmesh_find_path",
    "papyrus_call",
    "papyrus_native_call",
    "nif_parse",
    "dds_bc7_decode",
    "bsa_file_read",
];
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "soe-prf15-breaker3-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn divan_output() -> String {
    let mut text = String::new();
    for operation in OPERATIONS {
        writeln!(
            text,
            "├─ {operation} 1 ns │ 1 ns │ 100 ns │ 100 ns │ 30 │ 100"
        )
        .unwrap();
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
        .expect("baseline helper starts")
}

fn assert_bad_baseline(measured: &Path, baseline: &Path, expected: &str) {
    let result = run("check", measured, baseline);
    assert_eq!(result.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains(expected), "unexpected diagnostic: {stderr}");
    assert!(!stderr.contains("Traceback"), "unexpected traceback: {stderr}");
}

#[test]
fn prf_15_missing_baseline_median_is_rejected() {
    let measured = scratch("missing-median-output");
    let baseline = scratch("missing-median-baseline");
    fs::write(&measured, divan_output()).unwrap();
    assert!(run("refresh", &measured, &baseline).status.success());

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&baseline).unwrap()).unwrap();
    saved["operations"].as_object_mut().unwrap().remove("dds_bc7_decode");
    fs::write(&baseline, serde_json::to_vec(&saved).unwrap()).unwrap();

    assert_bad_baseline(&measured, &baseline, "missing positive baseline for dds_bc7_decode");
    let _ = fs::remove_file(measured);
    let _ = fs::remove_file(baseline);
}

#[test]
fn prf_15_missing_machine_identity_field_is_diagnosed() {
    let measured = scratch("missing-machine-output");
    let baseline = scratch("missing-machine-baseline");
    fs::write(&measured, divan_output()).unwrap();
    assert!(run("refresh", &measured, &baseline).status.success());

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&baseline).unwrap()).unwrap();
    saved["machine"].as_object_mut().unwrap().remove("architecture");
    fs::write(&baseline, serde_json::to_vec(&saved).unwrap()).unwrap();

    assert_bad_baseline(&measured, &baseline, "baseline machine must contain architecture");
    let _ = fs::remove_file(measured);
    let _ = fs::remove_file(baseline);
}

#[test]
fn prf_15_nonpositive_or_boolean_baseline_medians_are_rejected() {
    let measured = scratch("invalid-median-output");
    let baseline = scratch("invalid-median-baseline");
    fs::write(&measured, divan_output()).unwrap();
    assert!(run("refresh", &measured, &baseline).status.success());
    let original: serde_json::Value =
        serde_json::from_slice(&fs::read(&baseline).unwrap()).unwrap();

    for invalid in [serde_json::json!(0), serde_json::json!(true)] {
        let mut saved = original.clone();
        saved["operations"]["form_lookup"] = invalid;
        fs::write(&baseline, serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_bad_baseline(
            &measured,
            &baseline,
            "baseline operations must contain positive integer medians",
        );
    }

    let _ = fs::remove_file(measured);
    let _ = fs::remove_file(baseline);
}
