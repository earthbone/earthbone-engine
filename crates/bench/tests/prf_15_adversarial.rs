// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const OPS: [&str; 9] = [
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
const BATCHES: [u64; 9] = [1024, 1024, 1024, 128, 1, 1, 128, 1, 128];
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "soe-prf15-adversarial-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn output(per_operation_ns: u64, omitted: Option<&str>) -> String {
    let mut text = String::new();
    for (name, batch) in OPS.into_iter().zip(BATCHES) {
        if omitted == Some(name) {
            continue;
        }
        writeln!(
            text,
            "├─ {name} 1 ns │ 1 ns │ {} ns │ {} ns │ 30 │ 100",
            per_operation_ns * batch,
            per_operation_ns * batch
        )
        .unwrap();
    }
    text
}

fn run(mode: &str, output: &Path, baseline: &Path, data: bool) -> Output {
    let mut command = Command::new("python3");
    command
        .arg(root().join("tools/perf_baselines.py"))
        .arg(mode)
        .arg("--output")
        .arg(output)
        .arg("--baseline")
        .arg(baseline)
        .env_remove("SKYRIM_DATA");
    if data {
        command.env("SKYRIM_DATA", "fixture-data-dir");
    }
    command.output().expect("baseline helper starts")
}

fn cleanup(paths: &[&Path]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn prf_15_accepts_exactly_fifteen_percent_and_rejects_any_more() {
    let measured = scratch("threshold-output");
    let baseline = scratch("threshold-baseline");
    fs::write(&measured, output(100, None)).unwrap();
    assert!(run("refresh", &measured, &baseline, false).status.success());

    fs::write(&measured, output(115, None)).unwrap();
    let exact = run("check", &measured, &baseline, false);
    assert!(
        exact.status.success(),
        "{}",
        String::from_utf8_lossy(&exact.stderr)
    );
    fs::write(&measured, output(116, None)).unwrap();
    assert_eq!(
        run("check", &measured, &baseline, false).status.code(),
        Some(1)
    );
    cleanup(&[&measured, &baseline]);
}

#[test]
fn prf_15_missing_measurement_fails_without_replacing_existing_baseline() {
    let measured = scratch("missing-output");
    let baseline = scratch("missing-baseline");
    fs::write(&measured, output(100, None)).unwrap();
    assert!(run("refresh", &measured, &baseline, false).status.success());
    let before = fs::read(&baseline).unwrap();

    fs::write(&measured, output(100, Some("dds_bc7_decode"))).unwrap();
    let result = run("refresh", &measured, &baseline, false);
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(fs::read(&baseline).unwrap(), before);
    cleanup(&[&measured, &baseline]);
}

#[test]
fn prf_15_installed_mode_requires_both_asset_measurements() {
    let measured = scratch("installed-output");
    let baseline = scratch("installed-baseline");
    fs::write(&measured, output(100, None)).unwrap();
    let result = run("refresh", &measured, &baseline, true);
    assert_eq!(result.status.code(), Some(2));
    assert!(!baseline.exists());
    let message = String::from_utf8_lossy(&result.stderr);
    assert!(message.contains("installed_bsa_file_read"));
    assert!(message.contains("installed_nif_parse"));
    cleanup(&[&measured, &baseline]);
}

#[test]
fn prf_15_installed_mesh_asset_can_be_read_repeatedly_and_parsed() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let mut archives: Vec<_> = fs::read_dir(data)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bsa"))
                && path.file_name().is_some_and(|name| {
                    name.to_string_lossy().to_ascii_lowercase().contains("mesh")
                })
        })
        .collect();
    archives.sort();
    let archive = soe_bsa::archive::Archive::open(archives.first().expect("mesh BSA exists"))
        .expect("installed mesh archive opens");
    let name = archive
        .files()
        .iter()
        .find(|name| name.to_ascii_lowercase().ends_with(".nif"))
        .expect("mesh archive has a NIF");
    let first = archive.read(name).expect("first archive read succeeds");
    let second = archive.read(name).expect("repeated archive read succeeds");
    assert_eq!(first, second);
    assert!(
        soe_nif::parse(&first).is_ok(),
        "installed mesh {name} parses"
    );
}

#[test]
fn prf_15_baseline_rejects_a_non_object_operations_record_cleanly() {
    let measured = scratch("invalid-output");
    let baseline = scratch("invalid-baseline");
    fs::write(&measured, output(100, None)).unwrap();
    assert!(run("refresh", &measured, &baseline, false).status.success());
    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&baseline).unwrap()).unwrap();
    saved["operations"] = serde_json::Value::Array(Vec::new());
    fs::write(&baseline, serde_json::to_vec(&saved).unwrap()).unwrap();
    let result = run("check", &measured, &baseline, false);
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        !stderr.contains("Traceback"),
        "unexpected traceback: {stderr}"
    );
    cleanup(&[&measured, &baseline]);
}
