// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

fn report_count(report: &str, field: &str) -> f64 {
    let marker = format!("\"{field}\":");
    let value = report
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing {field} in Godot report: {report}"))
        .1
        .split([',', '}'])
        .next()
        .expect("report value")
        .trim();
    value
        .parse()
        .unwrap_or_else(|_| panic!("invalid {field} value {value}"))
}

#[test]
fn prf_30_resident_cell_render_counts_reconcile_with_the_streamer() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate parent")
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let project = root.join("godot");
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .arg("--resolution")
        .arg("640x360")
        .arg("--script")
        .arg("prf_30_godot_render_report_test.gd")
        .env("SKYRIM_DATA", data)
        .output()
        .expect("run installed-data Godot report scene");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Godot report scene failed: stdout={stdout}\nstderr={stderr}"
    );
    let report = stdout
        .lines()
        .find_map(|line| line.strip_prefix("PRF_30_RENDER_REPORT "))
        .expect("installed-data report summary");
    let cell_instances = report_count(report, "cell_instances");
    let built = report_count(report, "built");
    let godot_objects = report_count(report, "godot_objects_in_frame");
    assert_eq!(
        cell_instances, built,
        "resident per-cell count differs from streamer; report={report}"
    );
    assert!(
        godot_objects > 0.0 && godot_objects <= cell_instances,
        "in-frame objects should be a culled subset of resident batches; report={report}"
    );
}
