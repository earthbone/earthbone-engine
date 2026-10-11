// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn hst_19_repeated_papyrus_capture_writes_png_and_bad_path_fails() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping HST-19 host check");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipping HST-19 host check");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let stem = format!("soe_hst19_breaker_{}", std::process::id());
    let first = workspace.join("target").join(format!("{stem}_first.png"));
    let second = workspace.join("target").join(format!("{stem}_second.png"));
    let script = workspace.join("target").join(format!("{stem}.gd"));
    let source = r#"extends SceneTree

const CELL := "WhiterunBanneredMare"
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("HST-19 FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "load installed data")
	_check(streamer.call("stream_cell", CELL) > 0, "stream cell visuals")
	_check(streamer.call("stream_cell_collision", CELL) > 0, "stream cell collision")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "share host data")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0, "enter cell")
	_check(player.call("start_gameplay"), "start game")
	await process_frame
	await process_frame
	var bad: bool = player.call("take_screenshot", "/no/such/soe-hst19-directory/capture.png")
	_check(not bad, "unwritable destination must report failure")
	var paths := [OS.get_environment("SOE_HST19_FIRST"), OS.get_environment("SOE_HST19_SECOND")]
	for path: String in paths:
		_check(player.call("take_screenshot", path), "Papyrus screenshot request failed: " + path)
		var image: Image = Image.load_from_file(path)
		_check(not image.is_empty(), "requested PNG is not decodable: " + path)
		_check(image.get_width() == 640 and image.get_height() == 360, "PNG dimensions do not match viewport")
		_check(FileAccess.get_file_as_bytes(path).slice(0, 8) == PackedByteArray([137, 80, 78, 71, 13, 10, 26, 10]), "PNG signature is invalid")
		DirAccess.remove_absolute(path)
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " hst_19 repeated capture and bad path")
	quit(0 if failures == 0 else 1)
"#;
    fs::write(&script, source).expect("write temporary Godot test script");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360"])
        .arg("--script")
        .arg(&script)
        .env("SKYRIM_DATA", data)
        .env("SOE_HST19_FIRST", &first)
        .env("SOE_HST19_SECOND", &second)
        .output()
        .expect("launch headless HST-19 host check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = fs::remove_file(&script);
    assert!(
        output.status.success()
            && stdout.contains("RESULT: PASS hst_19 repeated capture and bad path")
            && !stderr.contains("SCRIPT ERROR"),
        "HST-19 host check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
