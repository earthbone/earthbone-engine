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
fn prf_30_cell_render_counts_match_godots_objects_in_frame() {
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let data = std::env::var_os("SKYRIM_DATA").unwrap_or_else(|| {
        "/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data"
            .into()
    });
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
    let godot_objects = report_count(report, "godot_objects_in_frame");
    assert_eq!(
        cell_instances, godot_objects,
        "resident per-cell render counts must match Godot's in-frame objects; report={report}"
    );
}
