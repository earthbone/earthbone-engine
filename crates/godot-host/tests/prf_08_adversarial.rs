// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for PRF-08's cold-start acceptance evidence.

use std::path::Path;

fn workspace() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap()
}

#[test]
fn cold_start_children_use_the_windowed_whiterun_radius_one_workload() {
    let root = workspace();
    let runs = std::fs::read_to_string(root.join("godot/bench_runs.gd")).unwrap();
    let benchmark = std::fs::read_to_string(root.join("godot/bench.gd")).unwrap();
    let harness = include_str!("prf_08_cold_start.rs");

    assert!(runs.contains("\"--windowed\""));
    assert!(harness.contains("\"--cold-starts\", \"3\""));
    assert!(harness.contains("\"WhiterunOrigin\""));
    assert!(harness.contains("\"SOE_BENCH_RADIUS\", \"1\""));
    assert!(benchmark.contains("DisplayServer.get_name() == \"headless\""));
    assert!(benchmark.contains("godot_run %d worst %.3f ms frame %d"));
}

#[test]
fn windowed_acceptance_validates_pipeline_cause_telemetry() {
    let harness = include_str!("prf_08_cold_start.rs");
    let warmup = std::fs::read_to_string(workspace().join("godot/render_warmup.gd")).unwrap();

    assert!(warmup.contains("pipelines_before_render"));
    assert!(warmup.contains("pipelines_after_load"));
    assert!(harness.contains("pipelines_during_measurement"));
}
