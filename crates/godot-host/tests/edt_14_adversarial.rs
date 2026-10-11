// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::Transform;
use soe_esm::{Plugin, Tag};

#[test]
fn edt_14_gizmo_bridge_edits_record_undoes_proxy_and_saves_restored_pose() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: set SOE_GODOT_BIN to a Godot 4.4 or newer executable");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
    // CONTEXT: Cargo test does not refresh the release library Godot loads.
    let build = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args(["build", "-p", "soe-godot-host", "--release", "--target-dir"])
        .arg(workspace.join("target"))
        .output()
        .expect("release extension build starts");
    assert!(
        build.status.success(),
        "release extension build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let fixture = tempfile::tempdir().unwrap();
    let base = fixture.path().join("Base.esm");
    let patch = fixture.path().join("Patch.esp");
    let original = Transform {
        position: [-2048.0, 12288.0, -17.5],
        rotation: [0.125, -1.75, 3.0],
    };
    fs::write(&base, Plugin::new(FileHeader::default()).to_bytes()).unwrap();
    let mut active = Plugin::new(FileHeader {
        masters: vec!["Base.esm".to_string()],
        ..Default::default()
    });
    active.push_top(
        Record::new(Tag(*b"REFR"), 0x0100_0800)
            .sub(Tag(*b"NAME"), 0x800u32.to_le_bytes().to_vec())
            .sub(Tag(*b"DATA"), original.to_bytes().to_vec()),
    );
    fs::write(&patch, active.to_bytes()).unwrap();

    let script = fixture.path().join("edt_14_bridge_test.gd");
    fs::write(
        &script,
        r#"extends SceneTree

const Target = preload("res://addons/soe_reference_gizmo/reference_target.gd")
const ReferenceHandle = preload("res://addons/soe_reference_gizmo/reference_handle.gd")

var failures := 0

func _check(value: bool, message: String) -> void:
	if not value:
		failures += 1
		printerr("FAIL: ", message)

func _initialize() -> void:
	await process_frame
	# CONTEXT: A fresh checkout has no cached extension list.
	var status := GDExtensionManager.load_extension("res://soe.gdextension")
	if status != GDExtensionManager.LOAD_STATUS_OK and status != GDExtensionManager.LOAD_STATUS_ALREADY_LOADED:
		printerr("FAIL: extension did not load: ", status)
		quit(1)
		return
	var paths := PackedStringArray([OS.get_environment("EDT14_BASE"), OS.get_environment("EDT14_PATCH")])
	var editor = ClassDB.instantiate("SoeReferenceTransformEditor")
	if editor == null:
		printerr("FAIL: transform editor class is unavailable")
		quit(1)
		return
	if not editor.open(paths, "Patch.esp"):
		printerr("FAIL: synthetic load order did not open: ", editor.last_error())
		quit(1)
		return
	var viewport := SubViewport.new()
	viewport.size = Vector2i(640, 360)
	root.add_child(viewport)
	var scene := Node3D.new()
	viewport.add_child(scene)
	var target = Target.new()
	target.form_id = "1:000800"
	target.editor_document = editor
	scene.add_child(target)
	if not target.load_record_transform():
		printerr("FAIL: proxy did not load its record transform")
		quit(1)
		return
	var before_position: Vector3 = target.global_position
	var before_rotation: Vector3 = target.record_rotation
	var restore := {"position": before_position, "rotation": before_rotation}
	var camera := Camera3D.new()
	scene.add_child(camera)
	camera.position = before_position + Vector3(0.0, 0.0, 1000.0)
	camera.look_at(before_position)
	camera.current = true
	await process_frame
	_check(ReferenceHandle.drag_target_to_screen(target, camera, Vector2(400.0, 180.0)), "editor handle ray intersects the placement plane")
	var after_position: Vector3 = target.global_position
	_check(after_position != before_position, "dragged handle moves the rendered reference proxy")
	var after_rotation := Vector3(-2.0, 0.75, 3.125)
	_check(ReferenceHandle.commit_target_transform(target, after_position, after_rotation, restore, false), "gizmo commit reaches the record bridge")
	var edited: Dictionary = editor.reference_transform(target.form_id)
	_check(edited.get("position") == after_position, "edited record has gizmo position")
	_check(edited.get("rotation") == after_rotation, "edited record has gizmo rotation")
	_check(editor.undo(), "record edit can be undone")
	_check(target.load_record_transform(), "proxy reloads after undo")
	_check(target.global_position == before_position, "undo restores proxy position")
	_check(target.record_rotation == before_rotation, "undo restores proxy rotation")
	_check(editor.save(), "restored plugin saves")
	var reopened = ClassDB.instantiate("SoeReferenceTransformEditor")
	_check(reopened != null and reopened.open(paths, "Patch.esp"), "saved plugin reopens")
	if reopened != null:
		var saved: Dictionary = reopened.reference_transform(target.form_id)
		_check(saved.get("position") == before_position, "saved placement restores position")
		_check(saved.get("rotation") == before_rotation, "saved placement restores rotation")
	print("EDT-14 ADVERSARIAL ", "PASS" if failures == 0 else "FAIL")
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
        .env("EDT14_BASE", base)
        .env("EDT14_PATCH", patch)
        .output()
        .expect("Godot editor process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("EDT-14 ADVERSARIAL PASS"),
        "EDT-14 bridge check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
