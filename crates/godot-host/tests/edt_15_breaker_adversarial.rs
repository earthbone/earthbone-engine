// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::Transform;
use soe_esm::{Plugin, Tag};

#[test]
fn edt_15_drop_requires_a_collision_and_keeps_successful_edit_undoable() {
    let godot = PathBuf::from(std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN"));
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
    let fixture = tempfile::tempdir().unwrap();
    let base = fixture.path().join("Base.esm");
    let patch = fixture.path().join("Patch.esp");
    fs::write(&base, Plugin::new(FileHeader::default()).to_bytes()).unwrap();
    let mut active = Plugin::new(FileHeader {
        masters: vec!["Base.esm".to_string()],
        ..Default::default()
    });
    let original = Transform {
        position: [0.0, 0.0, 100.0],
        rotation: [0.25, -0.5, 1.75],
    };
    active.push_top(
        Record::new(Tag(*b"REFR"), 0x0100_0800)
            .sub(Tag(*b"NAME"), 0x800u32.to_le_bytes().to_vec())
            .sub(Tag(*b"DATA"), original.to_bytes().to_vec()),
    );
    fs::write(&patch, active.to_bytes()).unwrap();

    let script = fixture.path().join("edt_15_bridge_test.gd");
    fs::write(
        &script,
        r#"extends SceneTree

var failures := 0

func _check(value: bool, message: String) -> void:
	if not value:
		failures += 1
		printerr("FAIL: ", message)

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	var paths := PackedStringArray([OS.get_environment("EDT15_BASE"), OS.get_environment("EDT15_PATCH")])
	var table := SoeFormTable.new()
	_check(table.open(paths, "Patch.esp"), "synthetic load order opens")
	var stage := Node3D.new()
	root.add_child(stage)
	var space: RID = stage.get_world_3d().space
	_check(not table.drop_selected_reference_to_ground(space, "1:000800"), "clear space rejects ground drop")
	_check(not table.last_error().is_empty(), "clear space reports why the drop failed")
	_check(not table.undo(), "failed collision query adds no undo edit")

	var shape := PhysicsServer3D.box_shape_create()
	PhysicsServer3D.shape_set_data(shape, Vector3(1.0, 0.1, 1.0))
	var body := PhysicsServer3D.body_create()
	PhysicsServer3D.body_set_mode(body, PhysicsServer3D.BODY_MODE_STATIC)
	PhysicsServer3D.body_add_shape(body, shape)
	PhysicsServer3D.body_set_space(body, space)
	await physics_frame
	_check(table.drop_selected_reference_to_ground(space, "1:000800"), "selected reference drops onto host collision")
	_check(table.last_error().is_empty(), "successful drop clears stale diagnostic")
	var placed: Dictionary = table.reference_transform("1:000800")
	var placed_position: Vector3 = placed.get("position", Vector3.INF)
	var placed_rotation: Vector3 = placed.get("rotation", Vector3.INF)
	_check(placed_position.is_finite(), "selected REFR record exposes its placed transform")
	_check(absf(placed_position.z - 7.0) < 0.1, "selected REFR record stores the host collision height")
	_check(absf(placed_position.x) < 0.001 and absf(placed_position.y) < 0.001, "ground drop preserves selected reference horizontal position")
	_check(placed_rotation == Vector3(0.25, -0.5, 1.75), "ground drop preserves selected reference rotation")
	_check(table.undo(), "successful collision edit is undoable")
	_check(not table.undo(), "undo stack is empty after restoring the original placement")
	PhysicsServer3D.free_rid(body)
	PhysicsServer3D.free_rid(shape)
	stage.free()
	print("EDT-15 ADVERSARIAL ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
"#,
    )
    .unwrap();
    let output = Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project)
        .arg("--script")
        .arg(script)
        .args(["--quit-after", "300"])
        .env("EDT15_BASE", base)
        .env("EDT15_PATCH", patch)
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("EDT-15 ADVERSARIAL PASS"),
        "EDT-15 bridge check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
