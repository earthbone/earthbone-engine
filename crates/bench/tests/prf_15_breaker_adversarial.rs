// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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
        "soe-prf15-breaker-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn output() -> String {
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

fn run(output: &Path, baseline: &Path) -> std::process::Output {
    Command::new("python3")
        .arg(root().join("tools/perf_baselines.py"))
        .arg("refresh")
        .arg("--output")
        .arg(output)
        .arg("--baseline")
        .arg(baseline)
        .env_remove("SKYRIM_DATA")
        .output()
        .expect("baseline helper runs")
}

#[test]
fn prf_15_duplicate_operation_medians_are_rejected_without_changing_baseline() {
    let measured = scratch("duplicate-output");
    let baseline = scratch("duplicate-baseline");
    fs::write(&measured, output()).unwrap();
    let initial = run(&measured, &baseline);
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let before = fs::read(&baseline).unwrap();

    let mut duplicate = output();
    writeln!(
        duplicate,
        "├─ form_lookup 1 ns │ 1 ns │ 999999 ns │ 999999 ns │ 30 │ 100"
    )
    .unwrap();
    fs::write(&measured, duplicate).unwrap();
    let result = run(&measured, &baseline);

    assert_eq!(result.status.code(), Some(2));
    assert_eq!(fs::read(&baseline).unwrap(), before);
    let _ = fs::remove_file(measured);
    let _ = fs::remove_file(baseline);
}
