// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
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
        "soe-prf15-breaker4-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn divan_output() -> String {
    let mut text = String::new();
    for (name, batch) in OPERATIONS {
        writeln!(
            text,
            "├─ {name} 1 ns │ 1 ns │ {} ns │ {} ns │ 30 │ 100",
            100 * batch,
            100 * batch
        )
        .unwrap();
    }
    text
}

#[test]
fn prf_15_boolean_format_version_is_rejected_as_malformed() {
    let measured = scratch("format-output");
    let baseline = scratch("format-baseline");
    fs::write(&measured, divan_output()).unwrap();
    let checker = root().join("tools/perf_baselines.py");
    let refresh = Command::new("python3")
        .arg(&checker)
        .arg("refresh")
        .arg("--output")
        .arg(&measured)
        .arg("--baseline")
        .arg(&baseline)
        .env_remove("SKYRIM_DATA")
        .output()
        .unwrap();
    assert!(refresh.status.success());

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&baseline).unwrap()).unwrap();
    saved["format"] = serde_json::Value::Bool(true);
    fs::write(&baseline, serde_json::to_vec(&saved).unwrap()).unwrap();
    let checked = Command::new("python3")
        .arg(checker)
        .arg("check")
        .arg("--output")
        .arg(&measured)
        .arg("--baseline")
        .arg(&baseline)
        .env_remove("SKYRIM_DATA")
        .output()
        .unwrap();

    assert_eq!(checked.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&checked.stderr);
    assert!(stderr.contains("baseline format must be 1"));
    assert!(!stderr.contains("Traceback"));
    let _ = fs::remove_file(measured);
    let _ = fs::remove_file(baseline);
}
