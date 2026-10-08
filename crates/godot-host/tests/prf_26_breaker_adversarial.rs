// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Visible and rejected menu transitions must be measured at the right boundary.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

#[test]
fn prf_26_menu_transition_timing_is_exposed_after_the_presented_frame() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let root = workspace();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let target = if target.is_absolute() {
        target
    } else {
        root.join(target)
    };
    let library = target.join("debug/deps/libsoe_godot_host.so");
    assert!(library.is_file(), "missing {}", library.display());
    let project = tempfile::tempdir_in(&root).unwrap();
    std::fs::write(
        project.path().join("project.godot"),
        "config_version=5\n[application]\nconfig/name=\"PRF-26 breaker\"\n",
    )
    .unwrap();
    std::fs::write(project.path().join("soe.gdextension"), format!("[configuration]\nentry_symbol=\"gdext_rust_init\"\ncompatibility_minimum=\"4.4\"\n[libraries]\nlinux.debug.x86_64=\"{}\"\n", library.display())).unwrap();
    std::fs::create_dir(project.path().join(".godot")).unwrap();
    std::fs::write(
        project.path().join(".godot/extension_list.cfg"),
        "res://soe.gdextension\n",
    )
    .unwrap();
    let transition_metrics =
        std::fs::read_to_string(root.join("godot/transition_metrics.gd")).unwrap();
    std::fs::write(
        project.path().join("transition_metrics.gd"),
        transition_metrics,
    )
    .unwrap();
    std::fs::write(
        project.path().join("check.gd"),
        r#"extends SceneTree
const TransitionMetrics := preload("res://transition_metrics.gd")
var metrics := TransitionMetrics.new()
func _initialize() -> void:
	call_deferred("_run")
func _run() -> void:
	var player := ClassDB.instantiate("PlayerNode") as Node3D
	root.add_child(player)
	player.transition_requested.connect(metrics.request)
	await process_frame
	var opened: bool = player.call("open_menu", "Inventory")
	var duplicate_open: bool = player.call("open_menu", "Magic")
	var closed: bool = player.call("close_menu")
	var duplicate_close: bool = player.call("close_menu")
	await RenderingServer.frame_post_draw
	await process_frame
	var failures: Array[String] = []
	if not opened or duplicate_open or not closed or duplicate_close:
		failures.append("menu action acceptance changed")
	var report: String = metrics.summary_text()
	if not report.contains("menu_open.Inventory 1") or not report.contains("menu_close.Inventory 1"):
		failures.append("successful open/close was not timed through first drawn frame")
	if report.contains("menu_open.Magic"):
		failures.append("rejected menu replacement was reported as a transition")
	if not report.contains("p50_ms worst_ms"):
		failures.append("transition summary lacks p50 and worst columns")
	if failures.is_empty(): quit(0)
	else:
		for failure in failures: printerr("FAIL: " + failure)
		quit(1)
"#,
    )
    .unwrap();
    let output = Command::new(godot)
        .args(["--path"])
        .arg(project.path())
        .args(["--resolution", "640x360", "--script", "check.gd"])
        .env("XDG_DATA_HOME", project.path())
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{log}");
}
