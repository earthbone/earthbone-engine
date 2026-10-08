// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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
fn prf_25_physics_report_exposes_body_controller_and_shape_build_metrics() {
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
        "config_version=5\n[application]\nconfig/name=\"PRF-25 breaker\"\n",
    )
    .unwrap();
    std::fs::write(project.path().join("soe.gdextension"), format!("[configuration]\nentry_symbol=\"gdext_rust_init\"\ncompatibility_minimum=\"4.4\"\n[libraries]\nlinux.debug.x86_64=\"{}\"\n", library.display())).unwrap();
    std::fs::create_dir(project.path().join(".godot")).unwrap();
    std::fs::write(
        project.path().join(".godot/extension_list.cfg"),
        "res://soe.gdextension\n",
    )
    .unwrap();
    std::fs::write(project.path().join("check.gd"), r#"extends SceneTree
func _initialize() -> void:
	call_deferred("_run")
func _run() -> void:
	var player := ClassDB.instantiate("PlayerNode") as Node3D
	root.add_child(player)
	await process_frame
	var report: Dictionary = player.call("physics_frame_breakdown")
	var missing: Array[String] = []
	for key in ["player_controller_us", "npc_controller_us", "ragdoll_bodies", "ragdoll_us", "dropped_item_bodies", "dropped_item_us", "collision_shape_build_us"]:
		if not report.has(key): missing.append(key)
	if missing.is_empty(): quit(0)
	else:
		printerr("missing physics report fields: " + str(missing))
		quit(1)
"#).unwrap();
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
