// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

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

fn run_godot(script: &str, windowed: bool) -> String {
    let root = workspace();
    let project = tempfile::tempdir_in(&root).expect("temporary project");
    std::fs::write(
        project.path().join("project.godot"),
        "config_version=5\n[application]\nconfig/name=\"HST-20 breaker\"\n",
    )
    .expect("project config");
    std::fs::copy(
        root.join("godot/render_warmup.gd"),
        project.path().join("render_warmup.gd"),
    )
    .expect("warm-up script");
    std::fs::write(project.path().join("check.gd"), script).expect("check script");

    let mut command = Command::new(godot_binary());
    if !windowed {
        command.arg("--headless");
    }
    let output = command
        .args(["--path"])
        .arg(project.path())
        .args(["--script", "check.gd"])
        .env("XDG_DATA_HOME", project.path())
        .output()
        .expect("run Godot check");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{log}");
    assert!(!log.contains("SCRIPT ERROR:"), "{log}");
    assert!(log.contains("RESULT: PASS"), "{log}");
    log
}

#[test]
fn long_undrawn_delay_keeps_full_timeout_and_begin_restarts_it() {
    run_godot(
        r#"extends SceneTree
const Warmup := preload("res://render_warmup.gd")
var failures := 0
func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr(message)
func _initialize() -> void:
	var wait := Warmup.new()
	var first := 10 + 77 * 60 * 1_000_000
	wait.begin(first, [0, 0, 0, 0, 0])
	check(not wait.timed_out(first + Warmup.TIMEOUT_USEC - 1), "timeout fires early")
	check(wait.timed_out(first + Warmup.TIMEOUT_USEC), "timeout misses exact boundary")
	var fields: Dictionary = Warmup.timing_fields(10, first, first + 500_000)
	check(fields.load_first_frame_wait_ms == 4_620_000.0, "first-frame delay is not reported separately")
	check(fields.load_render_ms == 500.0, "render duration excludes the first-frame wait")
	wait.begin(first + 9_000_000_000, [0, 0, 0, 0, 0])
	check(not wait.timed_out(first + 9_000_000_000 + Warmup.TIMEOUT_USEC - 1), "begin did not reset timeout")
	print("RESULT: %s" % ("PASS" if failures == 0 else "FAIL"))
	quit(0 if failures == 0 else 1)
"#,
        false,
    );
}

#[test]
fn drawn_frame_and_pipeline_quiet_boundaries_settle_only_after_three_frames() {
    run_godot(
        r#"extends SceneTree
const Warmup := preload("res://render_warmup.gd")
var failures := 0
func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr(message)
func _initialize() -> void:
	var wait := Warmup.new()
	var counts: Array[int] = [2, 4, 6, 8, 10]
	var first := 77 * 60 * 1_000_000
	wait.begin(first, counts)
	check(not wait.observe(first, counts), "one drawn frame settles")
	check(not wait.observe(first + 500_000, counts), "two drawn frames settle")
	var changed: Array[int] = [2, 4, 7, 8, 10]
	check(not wait.observe(first + 600_000, changed), "pipeline change settles immediately")
	check(not wait.observe(first + 1_099_999, changed), "quiet interval ends early")
	check(wait.observe(first + 1_100_000, changed), "three quiet drawn frames fail to settle")
	print("RESULT: %s" % ("PASS" if failures == 0 else "FAIL"))
	quit(0 if failures == 0 else 1)
"#,
        false,
    );
}

#[test]
fn windowed_prepare_returns_distinct_first_frame_and_render_durations() {
    let log = run_godot(
        r#"extends SceneTree
const Warmup := preload("res://render_warmup.gd")
func _initialize() -> void:
	call_deferred("_run")
func _run() -> void:
	var report: Dictionary = await Warmup.prepare()
	if not report.error.is_empty():
		printerr(report.error)
		quit(1)
		return
	var first_wait: float = report.get("load_first_frame_wait_ms", -1.0)
	var render: float = report.get("load_render_ms", -1.0)
	var worst: float = report.get("load_render_worst_ms", -1.0)
	if first_wait < 0.0 or render < 0.0 or worst < 0.0 or worst > render:
		printerr("invalid or overlapping timing fields: ", report)
		quit(1)
		return
	print("RESULT: PASS ", JSON.stringify(report))
	quit(0)
"#,
        true,
    );
    assert!(log.contains("load_first_frame_wait_ms"), "{log}");
    assert!(log.contains("load_render_ms"), "{log}");
    assert!(log.contains("load_render_worst_ms"), "{log}");
}
