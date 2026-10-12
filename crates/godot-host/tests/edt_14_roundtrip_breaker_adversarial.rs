// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::Transform;
use soe_esm::{Plugin, Tag};

#[test]
fn edt_14_repeated_gizmo_edits_undo_and_save_each_restored_pose() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: set SOE_GODOT_BIN to a Godot 4.4 or newer executable");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
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

    let script = fixture.path().join("edt_14_roundtrip_test.gd");
    fs::write(
        &script,
        r#"extends SceneTree

const Target = preload("res://addons/soe_reference_gizmo/reference_target.gd")
const Handle = preload("res://addons/soe_reference_gizmo/reference_handle.gd")

var failures := 0

func check(value: bool, message: String) -> void:
	if not value:
		failures += 1
		printerr("FAIL: ", message)

func reopen(paths: PackedStringArray):
	var document = ClassDB.instantiate("SoeReferenceTransformEditor")
	check(document != null and document.open(paths, "Patch.esp"), "saved document reopens")
	return document

func matches(document, id: String, position: Vector3, rotation: Vector3, label: String) -> void:
	var value: Dictionary = document.reference_transform(id)
	check(value.get("position") == position, label + " record position")
	check(value.get("rotation") == rotation, label + " record rotation")

func _initialize() -> void:
	await process_frame
	var status := GDExtensionManager.load_extension("res://soe.gdextension")
	if status != GDExtensionManager.LOAD_STATUS_OK and status != GDExtensionManager.LOAD_STATUS_ALREADY_LOADED:
		printerr("FAIL: extension did not load: ", status)
		quit(1)
		return
	var paths := PackedStringArray([OS.get_environment("EDT14_BASE"), OS.get_environment("EDT14_PATCH")])
	var editor = reopen(paths)
	if editor == null:
		quit(1)
		return
	var scene := Node3D.new()
	root.add_child(scene)
	var target = Target.new()
	target.form_id = "1:000800"
	target.editor_document = editor
	scene.add_child(target)
	check(target.load_record_transform(), "initial reference placement loads")
	var original_position := Vector3(-2048.0, 12288.0, -17.5)
	var original_rotation := Vector3(0.125, -1.75, 3.0)
	var first_position := Vector3(10.25, -20.5, 30.75)
	var first_rotation := Vector3(-0.5, 1.25, -2.75)
	var second_position := Vector3(-40.0, 50.0, -60.0)
	var second_rotation := Vector3(2.5, -1.5, 0.25)
	var first_restore := {"position": original_position, "rotation": original_rotation}
	check(Handle.commit_target_transform(target, first_position, first_rotation, first_restore, false), "first transform commits")
	var second_restore := {"position": first_position, "rotation": first_rotation}
	check(Handle.commit_target_transform(target, second_position, second_rotation, second_restore, false), "second transform commits")
	matches(editor, target.form_id, second_position, second_rotation, "second edit")
	check(editor.undo(), "first undo is available")
	check(target.load_record_transform(), "proxy reloads after first undo")
	check(target.global_position == first_position and target.record_rotation == first_rotation, "first undo restores rendered pose")
	matches(editor, target.form_id, first_position, first_rotation, "first undo")
	check(editor.save(), "first restored pose saves")
	var saved_first = reopen(paths)
	if saved_first != null:
		matches(saved_first, target.form_id, first_position, first_rotation, "saved first pose")
	check(editor.undo(), "second undo is available")
	check(target.load_record_transform(), "proxy reloads after second undo")
	check(target.global_position == original_position and target.record_rotation == original_rotation, "second undo restores original rendered pose")
	matches(editor, target.form_id, original_position, original_rotation, "second undo")
	check(editor.save(), "original pose saves")
	var saved_original = reopen(paths)
	if saved_original != null:
		matches(saved_original, target.form_id, original_position, original_rotation, "saved original pose")
	print("EDT-14 ROUNDTRIP BREAKER ", "PASS" if failures == 0 else "FAIL")
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
        output.status.success() && stdout.contains("EDT-14 ROUNDTRIP BREAKER PASS"),
        "EDT-14 repeated edit check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
