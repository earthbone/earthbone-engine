// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
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
        "soe-prf15-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn divan_output(ns: u64) -> String {
    let mut output = String::new();
    for (name, batch) in OPS.into_iter().zip(BATCHES) {
        let _ = writeln!(
            output,
            "├─ {name} 1 ns │ 1 ns │ {} ns │ {} ns │ 30 │ 100",
            ns * batch,
            ns * batch
        );
    }
    output
}

fn runner(
    mode: &str,
    output: &PathBuf,
    baseline: &PathBuf,
    include_installed: bool,
) -> std::process::Output {
    let mut command = Command::new("python3");
    command
        .arg(root().join("tools/perf_baselines.py"))
        .arg(mode)
        .arg("--output")
        .arg(output)
        .arg("--baseline")
        .arg(baseline)
        .env_remove("SKYRIM_DATA");
    if include_installed {
        command.env("SKYRIM_DATA", "installed-fixture");
    }
    command.output().expect("the Python baseline helper runs")
}

#[test]
fn prf_15_benchmark_target_covers_all_required_operations() {
    let source = fs::read_to_string(root().join("crates/bench/benches/prf_15.rs")).unwrap();
    for operation in OPS {
        assert!(
            source.contains(&format!("fn {operation}(")),
            "missing {operation}"
        );
    }
    assert!(source.contains("SKYRIM_DATA"));
    assert!(source.contains("BC7"));
    assert!(source.contains("Archive::open"));
    let manifest = fs::read_to_string(root().join("crates/bench/Cargo.toml")).unwrap();
    assert!(manifest.contains("divan = \"0.1.21\""));
    assert!(manifest.contains("name = \"prf_15\""));
}

#[test]
fn prf_15_baseline_enforces_machine_identity_and_fifteen_percent_limit() {
    let output_path = scratch("output");
    let baseline_path = scratch("baseline");
    fs::write(&output_path, divan_output(100)).unwrap();
    let refreshed = runner("refresh", &output_path, &baseline_path, false);
    assert!(
        refreshed.status.success(),
        "{}",
        String::from_utf8_lossy(&refreshed.stderr)
    );
    let accepted = runner("check", &output_path, &baseline_path, false);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    fs::write(&output_path, divan_output(116)).unwrap();
    let rejected = runner("check", &output_path, &baseline_path, false);
    assert_eq!(rejected.status.code(), Some(1));
    let mut baseline: serde_json::Value =
        serde_json::from_slice(&fs::read(&baseline_path).unwrap()).unwrap();
    baseline["machine"]["cpu"] = "different CPU".into();
    fs::write(&baseline_path, serde_json::to_vec(&baseline).unwrap()).unwrap();
    fs::write(&output_path, divan_output(100)).unwrap();
    let rejected = runner("check", &output_path, &baseline_path, false);
    assert_eq!(rejected.status.code(), Some(1));
    let _ = fs::remove_file(output_path);
    let _ = fs::remove_file(baseline_path);
}

#[test]
fn prf_15_baseline_includes_installed_medians_when_game_data_is_set() {
    let output_path = scratch("installed-output");
    let baseline_path = scratch("installed-baseline");
    let mut output = divan_output(100);
    for name in ["installed_nif_parse", "installed_bsa_file_read"] {
        let _ = writeln!(output, "├─ {name} 1 ns │ 1 ns │ 100 ns │ 100 ns │ 30 │ 100");
    }
    fs::write(&output_path, output).unwrap();
    let refreshed = runner("refresh", &output_path, &baseline_path, true);
    assert!(
        refreshed.status.success(),
        "{}",
        String::from_utf8_lossy(&refreshed.stderr)
    );
    let baseline: serde_json::Value =
        serde_json::from_slice(&fs::read(&baseline_path).unwrap()).unwrap();
    assert_eq!(baseline["data_mode"], "installed");
    assert_eq!(baseline["operations"].as_object().unwrap().len(), 11);
    let accepted = runner("check", &output_path, &baseline_path, true);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let _ = fs::remove_file(output_path);
    let _ = fs::remove_file(baseline_path);
}

#[test]
fn prf_15_installed_mesh_archive_asset_is_read_and_parsed_when_available() {
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
    let archive = soe_bsa::archive::Archive::open(archives.first().expect("mesh archive exists"))
        .expect("mesh archive opens");
    let name = archive
        .files()
        .iter()
        .find(|name| name.to_ascii_lowercase().ends_with(".nif"))
        .expect("mesh archive contains a NIF");
    let bytes = archive.read(name).expect("installed mesh reads");
    assert!(
        soe_nif::parse(&bytes).is_ok(),
        "installed NIF {name} parses"
    );
}
