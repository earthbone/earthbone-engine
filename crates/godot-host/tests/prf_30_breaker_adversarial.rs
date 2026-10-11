// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

fn run_godot(script: &str) -> Option<(bool, String)> {
    let (Some(godot), Some(data)) = (
        std::env::var_os("SOE_GODOT_BIN"),
        std::env::var_os("SKYRIM_DATA"),
    ) else {
        eprintln!("SKIP PRF-30: SOE_GODOT_BIN or SKYRIM_DATA is unset");
        return None;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .expect("crate parent")
        .parent()
        .expect("workspace root");
    let project = root.join("godot");
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args(["--resolution", "640x360", "--script", script])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot report scene should start");
    Some((
        output.status.success(),
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    ))
}

#[test]
fn prf_30_installed_render_report_reconciles_cells_with_godot_objects() {
    let Some((success, output)) = run_godot("prf_30_godot_render_report_test.gd") else {
        return;
    };
    assert!(success, "installed render report failed: {output}");
    let report = output
        .lines()
        .find_map(|line| line.strip_prefix("PRF_30_RENDER_REPORT "))
        .expect("installed report summary");
    let report: serde_json::Value = serde_json::from_str(report).expect("valid render report JSON");
    let cells = report["cell_instances"].as_f64().expect("cell instances");
    let objects = report["godot_objects_in_frame"]
        .as_f64()
        .expect("Godot objects in frame");
    let built = report["built"].as_f64().expect("built instances");
    assert_eq!(
        cells, built,
        "resident cell instances do not reconcile with streamer: {report}"
    );
    assert!(
        objects > 0.0 && objects <= cells,
        "in-frame objects must be a culled subset of resident batches: {report}"
    );
}

#[test]
fn prf_30_render_report_attributes_each_cull_stage_and_lod_switches() {
    let Some((success, output)) = run_godot("prf_30_godot_render_report_test.gd") else {
        return;
    };
    assert!(success, "installed render report failed: {output}");
    let report = output
        .lines()
        .find_map(|line| line.strip_prefix("SOE_FRAME_REPORT "))
        .expect("exit frame report");
    let report: serde_json::Value = serde_json::from_str(report).expect("valid frame report JSON");
    let attribution = &report["godot"]["render_attribution"];
    for field in [
        "frustum_culled",
        "occlusion_culled",
        "visibility_range_culled",
        "lod_culled",
        "unloaded_cell_culled",
        "lod_switches_per_second",
        "shadow_casters_per_light",
        "materials_created_per_frame",
        "meshes_created_per_frame",
    ] {
        let value = attribution
            .get(field)
            .unwrap_or_else(|| panic!("render attribution omits {field}: {report}"));
        if value.is_null() {
            assert!(
                attribution["unavailable"][field]
                    .as_str()
                    .is_some_and(|reason| !reason.is_empty()),
                "unmeasured {field} lacks an availability reason: {report}"
            );
        } else {
            assert!(
                !attribution["unavailable"]
                    .as_object()
                    .expect("availability reasons")
                    .contains_key(field),
                "measured {field} is also marked unavailable: {report}"
            );
        }
    }
    let sources = attribution
        .get("sources")
        .and_then(serde_json::Value::as_object)
        .expect("measurement sources");
    for source in [
        "performance_monitors",
        "viewport_passes",
        "render_time_ms",
        "cell_plugin_batches",
    ] {
        assert!(
            sources.get(source).is_some_and(|value| value.is_string()),
            "render report omits the source for {source}: {report}"
        );
    }
    assert_eq!(
        sources["viewport_passes"],
        "RenderingServer.viewport_get_render_info"
    );
    assert_eq!(
        sources["cell_plugin_batches"],
        "CellStreamerNode.render_attribution"
    );
}

#[test]
fn prf_30_report_reads_every_builtin_performance_monitor() {
    let Some((success, output)) = run_godot("prf_30_godot_render_report_test.gd") else {
        return;
    };
    assert!(success, "installed render report failed: {output}");
    let report = output
        .lines()
        .find_map(|line| line.strip_prefix("SOE_FRAME_REPORT "))
        .expect("exit frame report");
    let report: serde_json::Value = serde_json::from_str(report).expect("valid frame report JSON");
    let godot = report.get("godot").expect("Godot metrics");
    for field in [
        "fps",
        "static_memory_max_bytes",
        "message_buffer_max_bytes",
        "resources",
        "render_buffer_memory_bytes",
        "physics_2d_active_objects",
        "physics_2d_collision_pairs",
        "physics_2d_islands",
        "navigation_active_maps",
        "navigation_regions",
        "navigation_agents",
        "navigation_links",
        "navigation_polygons",
        "navigation_edges",
        "navigation_edge_merges",
        "navigation_edge_connections",
        "navigation_free_edges",
        "navigation_obstacles",
    ] {
        assert!(
            godot.get(field).is_some(),
            "frame report omits Godot monitor {field}: {godot}"
        );
    }
}
