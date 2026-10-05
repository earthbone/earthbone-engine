// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "common/isolated_project.rs"]
mod isolated_project;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate directory")
        .parent()
        .expect("workspace directory")
        .to_owned()
}

fn godot_binary() -> PathBuf {
    std::env::var_os("SOE_GODOT_BIN")
        .map(PathBuf::from)
        .expect("SOE_GODOT_BIN must point to the provided Godot runtime")
}

#[test]
fn hst_20_timeout_and_report_edges_run_against_godot() {
    let root = workspace();
    let project = tempfile::tempdir_in(&root).expect("temporary project");
    std::fs::write(
        project.path().join("project.godot"),
        "config_version=5\n[application]\nconfig/name=\"HST-20 adversarial\"\n",
    )
    .expect("project config");
    std::fs::copy(
        root.join("godot/render_warmup.gd"),
        project.path().join("render_warmup.gd"),
    )
    .expect("warm-up script");
    std::fs::write(
        project.path().join("hst_20_edges.gd"),
        r#"extends SceneTree
const Warmup := preload("res://render_warmup.gd")
var failures := 0

func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr(message)

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	var wait := Warmup.new()
	var counts: Array[int] = [1, 2, 3, 4, 5]
	var prepare_started := 10
	var first_drawn := prepare_started + 77 * 60 * 1_000_000
	wait.begin(first_drawn, counts)
	check(not wait.timed_out(first_drawn + Warmup.TIMEOUT_USEC - 1), "late first frame retains the full timeout")
	check(wait.timed_out(first_drawn + Warmup.TIMEOUT_USEC), "unsettled run fails at the 30 second boundary")

	wait.begin(first_drawn, counts)
	check(not wait.observe(first_drawn, counts), "one drawn frame cannot settle")
	check(not wait.observe(first_drawn + 250_000, counts), "two drawn frames cannot settle")
	var changed := counts.duplicate()
	changed[2] += 1
	check(not wait.observe(first_drawn + 400_000, changed), "pipeline changes reset the quiet interval")
	check(not wait.observe(first_drawn + 899_999, changed), "quiet interval must reach 500 ms")
	check(wait.observe(first_drawn + 900_000, changed), "three or more quiet drawn frames settle")
	check(not wait.timed_out(first_drawn + 900_000), "settled run succeeds after the long initial delay")

	wait.begin(first_drawn + 9_000_000_000, counts)
	check(not wait.timed_out(first_drawn + 9_000_000_000 + Warmup.TIMEOUT_USEC - 1), "begin resets the timeout origin")
	check(not wait.observe(first_drawn + 9_000_000_000, counts), "begin resets the drawn-frame count")
	var timing: Dictionary = Warmup.timing_fields(prepare_started, first_drawn, first_drawn + 900_000)
	check(timing.load_first_frame_wait_ms == 4_620_000.0, "first-frame delay is reported separately")
	check(timing.load_render_ms == 900.0, "render time starts at the first drawn frame")
	var zero_timing: Dictionary = Warmup.timing_fields(20, 20, 20)
	check(zero_timing.load_first_frame_wait_ms == 0.0 and zero_timing.load_render_ms == 0.0, "zero-duration report fields stay zero")
	print("RESULT: %s: hst_20 adversarial" % ("PASS" if failures == 0 else "FAIL"))
	quit(0 if failures == 0 else 1)
"#,
    )
    .expect("edge test script");

    let output = Command::new(godot_binary())
        .args(["--headless", "--path"])
        .arg(project.path())
        .args(["--script", "hst_20_edges.gd"])
        .env("XDG_DATA_HOME", project.path())
        .output()
        .expect("run headless Godot edge checks");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{log}");
    assert!(!log.contains("SCRIPT ERROR:"), "{log}");
    assert!(log.contains("RESULT: PASS: hst_20 adversarial"), "{log}");
}

#[test]
fn hst_20_windowed_prepare_reports_both_real_intervals() {
    let project = isolated_project::project(
        &[
            "render_warmup.gd",
            "bench_metrics.gd",
            "bench_runs.gd",
            "prf_08_test.gd",
        ],
        None,
    );
    let output = Command::new(godot_binary())
        .args(["--path"])
        .arg(project.path())
        .args(["--resolution", "640x360", "--script", "prf_08_test.gd"])
        .output()
        .expect("run windowed PRF-08 checks");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{log}");
    assert!(output.status.success(), "{log}");
    assert!(!log.contains("SCRIPT ERROR:"), "{log}");
    assert!(
        log.contains("RESULT: PASS: prf_08 deterministic checks"),
        "{log}"
    );
}
