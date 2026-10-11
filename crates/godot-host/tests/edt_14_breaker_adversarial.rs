// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::Transform;
use soe_esm::{Plugin, Tag};

#[test]
fn edt_14_cancel_and_rejected_gizmo_edits_preserve_record_and_proxy() {
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
    active.push_top(
        Record::new(Tag(*b"REFR"), 0x0100_0801)
            .sub(Tag(*b"NAME"), 0x800u32.to_le_bytes().to_vec()),
    );
    fs::write(&patch, active.to_bytes()).unwrap();

    let script = fixture.path().join("edt_14_cancel_test.gd");
    fs::write(
        &script,
        r#"extends SceneTree

const Target = preload("res://addons/soe_reference_gizmo/reference_target.gd")
const ReferenceHandle = preload("res://addons/soe_reference_gizmo/reference_handle.gd")

var failures := 0

func check(value: bool, message: String) -> void:
	if not value:
		failures += 1
		printerr("FAIL: ", message)

func _initialize() -> void:
	await process_frame
	var status := GDExtensionManager.load_extension("res://soe.gdextension")
	if status != GDExtensionManager.LOAD_STATUS_OK and status != GDExtensionManager.LOAD_STATUS_ALREADY_LOADED:
		printerr("FAIL: extension did not load: ", status)
		quit(1)
		return
	var paths := PackedStringArray([OS.get_environment("EDT14_BASE"), OS.get_environment("EDT14_PATCH")])
	var editor = ClassDB.instantiate("SoeReferenceTransformEditor")
	check(editor != null and editor.open(paths, "Patch.esp"), "synthetic document opens")
	if editor == null or editor.reference_transform("1:000800").is_empty():
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
	check(target.load_record_transform(), "valid source placement loads")
	var start: Vector3 = target.global_position
	var rotation: Vector3 = target.record_rotation
	var restore := {"position": start, "rotation": rotation}
	var camera := Camera3D.new()
	scene.add_child(camera)
	camera.position = start + Vector3(0.0, 0.0, 1000.0)
	camera.look_at(start)
	camera.current = true
	await process_frame
	check(ReferenceHandle.drag_target_to_screen(target, camera, Vector2(400.0, 180.0)), "valid screen ray moves preview")
	check(target.global_position != start, "drag changes proxy position")
	check(editor.reference_transform(target.form_id).position == start, "uncommitted drag leaves record unchanged")
	check(ReferenceHandle.commit_target_transform(target, target.global_position, rotation, restore, true), "cancel is accepted")
	check(target.global_position == start and target.record_rotation == rotation, "cancel restores proxy pose")
	check(editor.reference_transform(target.form_id).position == start and not editor.undo(), "cancel creates no record edit")
	var invalid = Target.new()
	invalid.form_id = "1:000801"
	invalid.editor_document = editor
	scene.add_child(invalid)
	invalid.global_position = Vector3(8.0, 9.0, 10.0)
	var invalid_restore := {"position": invalid.global_position, "rotation": Vector3.ZERO}
	check(not ReferenceHandle.commit_target_transform(invalid, Vector3.ONE, Vector3.ONE, invalid_restore, false), "missing DATA edit is rejected")
	check(invalid.global_position == invalid_restore.position, "rejected edit restores proxy")
	var absent = Target.new()
	absent.form_id = "1:000801"
	absent.editor_document = editor
	scene.add_child(absent)
	absent.global_position = Vector3(3.0, 4.0, 5.0)
	check(not absent.load_record_transform(), "missing DATA cannot load as a placement")
	check(absent.global_position == Vector3(3.0, 4.0, 5.0), "failed load leaves proxy untouched")
	print("EDT-14 BREAKER ", "PASS" if failures == 0 else "FAIL")
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
        output.status.success() && stdout.contains("EDT-14 BREAKER PASS"),
        "EDT-14 cancel and rejection check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
