// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Atomicity check for failed restored-host attachment.

use std::path::PathBuf;
use std::process::Command;

fn installed_godot() -> Option<(PathBuf, PathBuf)> {
    let godot = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from)?;
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    assert!(
        target.join("release/libsoe_godot_host.so").is_file(),
        "build the release GDExtension first"
    );
    assert!(data.is_dir(), "SKYRIM_DATA must point to installed Skyrim data");
    Some((godot, data))
}

#[test]
fn sav_19_verified_attach_installs_cell_camera_and_advances_frame() {
    let Some((godot, data)) = installed_godot() else {
        eprintln!("skipping: set SOE_GODOT_BIN and SKYRIM_DATA for the installed-data check");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let source = project.join("sav_19_host_attach_test.gd");
    let script = std::fs::read_to_string(source).unwrap();
    let script = script
        .replace("var saved :=", "var saved: Dictionary =")
        .replace("var rejected :=", "var rejected: Dictionary =")
        .replace("var attached :=", "var attached: Dictionary =");
    let save_dir = tempfile::tempdir().unwrap();
    let script_dir = tempfile::tempdir().unwrap();
    let script_path = script_dir.path().join("sav_19_host_attach_test.gd");
    std::fs::write(&script_path, script).unwrap();
    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--script"])
        .arg(script_path)
        .args(["--resolution", "640x360"])
        .env("SKYRIM_DATA", data)
        .env("SAV_19_SAVE_DIR", save_dir.path())
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS sav_19 host attach"),
        "SAV-19 successful host attach failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn sav_19_corrupt_generation_keeps_active_game_and_host_state() {
    let Some((godot, data)) = installed_godot() else {
        eprintln!("skipping: set SOE_GODOT_BIN and SKYRIM_DATA for the installed-data check");
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let script = r#"
extends SceneTree

var failures := 0

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _player(streamer: Node3D) -> Node3D:
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "player did not share installed data")
	return player

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "installed data did not load")
	var source := _player(streamer)
	_check(source.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "source cell did not load")
	_check(source.call("start_gameplay"), "source Game did not start")
	var directory := OS.get_environment("SAV_19_SAVE_DIR")
	var saved: Dictionary = source.call("save_game_generation", directory, 2)
	_check(saved.get("ok", false), "source generation did not save")
	var path := directory.path_join("generation-00000000000000000001.soeb")
	var bytes := FileAccess.get_file_as_bytes(path)
	_check(bytes.size() > 8, "saved generation is missing or too short")
	if bytes.size() > 8:
		bytes[bytes.size() - 1] = bytes[bytes.size() - 1] ^ 0xff
		var output := FileAccess.open(path, FileAccess.WRITE)
		_check(output != null, "corrupt generation could not be written")
		if output != null:
			output.store_buffer(bytes)
			output.close()

	var active := _player(streamer)
	_check(active.call("enter_cell", "WhiterunBanneredMare", 40.0, 0.0, 500.0) > 0, "active cell did not load")
	_check(active.call("start_gameplay"), "active Game did not start")
	active.call("set_facing", -0.4, 0.2)
	var cell_before: String = active.call("gameplay_cell")
	var position_before: Vector3 = active.call("position")
	var facing_before: Vector2 = active.call("facing")
	var days_before: float = active.call("gameplay_days")
	var result: Dictionary = active.call("attach_restored_generation", directory, 2)
	_check(not result.get("ok", true), "corrupt generation was accepted")
	_check(not result.get("state_loaded", true), "corrupt generation reported loaded state")
	_check(active.call("gameplay_cell") == cell_before, "failed import replaced the active cell")
	_check(active.call("position").distance_to(position_before) < 0.001, "failed import changed the active pose")
	_check(active.call("facing").distance_to(facing_before) < 0.001, "failed import changed the active camera")
	_check(is_equal_approx(active.call("gameplay_days"), days_before), "failed import changed the active Game")
	active.free()
	source.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " sav_19 atomic recovery")
	quit(0 if failures == 0 else 1)
"#;
    let temp = tempfile::tempdir().unwrap();
    let script_path = temp.path().join("sav_19_corrupt_generation.gd");
    std::fs::write(&script_path, script).unwrap();
    let save_dir = tempfile::tempdir().unwrap();
    let output = Command::new(godot)
        .args(["--path"])
        .arg(project)
        .args(["--script"])
        .arg(script_path)
        .args(["--resolution", "640x360"])
        .env("SKYRIM_DATA", data)
        .env("SAV_19_SAVE_DIR", save_dir.path())
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS sav_19 atomic recovery"),
        "SAV-19 atomic recovery failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
