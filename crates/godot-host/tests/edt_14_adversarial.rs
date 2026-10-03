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

var failures := 0

func _check(value: bool, message: String) -> void:
	if not value:
		failures += 1
		printerr("FAIL: ", message)

func _initialize() -> void:
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
	var target = Target.new()
	target.form_id = "1:000800"
	target.editor_document = editor
	if not target.load_record_transform():
		printerr("FAIL: proxy did not load its record transform")
		quit(1)
		return
	var before_position: Vector3 = target.position
	var before_rotation: Vector3 = target.rotation
	var restore := {"position": before_position, "rotation": before_rotation}
	var after_position := Vector3(-3900.0, 8192.0, -4096.25)
	var after_rotation := Vector3(-2.0, 0.75, 3.125)
	_check(target.commit_gizmo_transform(after_position, after_rotation, restore, false), "gizmo commit reaches the record bridge")
	var edited: Dictionary = editor.reference_transform(target.form_id)
	_check(edited.get("position") == after_position, "edited record has gizmo position")
	_check(edited.get("rotation") == after_rotation, "edited record has gizmo rotation")
	_check(editor.undo(), "record edit can be undone")
	_check(target.load_record_transform(), "proxy reloads after undo")
	_check(target.position == before_position, "undo restores proxy position")
	_check(target.rotation == before_rotation, "undo restores proxy rotation")
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
