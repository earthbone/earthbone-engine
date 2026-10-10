// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Output};

fn assert_godot_passed(output: &Output, marker: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains(marker), "{stdout}\n{stderr}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}

#[test]
fn ui_17_breaker3_chargen_native_changes_live_streamer_residency_and_bytes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data = std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA must name installed Skyrim data");
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must name Godot 4.4");
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("ui_17_breaker3.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree

var failures := 0

func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _initialize() -> void:
	call_deferred("run_checks")

func run_checks() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	check(streamer.call("load_data", data), "installed data did not load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	check(player.call("use_streamer_data", streamer), "player did not share streamer data")
	check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"gameplay cell did not load")
	check(player.call("start_gameplay"), "game runtime did not start")
	check(not player.call("is_menu_open"), "test unexpectedly opened a menu")
	check(streamer.call("chargen_resident_count") == 0, "chargen files were resident before precache")
	check(player.call("debug_invoke_chargen_native", "PrecacheCharGen"), "precache native did not run")
	var resident := int(streamer.call("chargen_resident_count"))
	var models := int(streamer.call("chargen_model_count"))
	var morphs := int(streamer.call("chargen_morph_count"))
	var bytes := int(streamer.call("chargen_resident_bytes"))
	check(resident > 0 and models > 0 and morphs > 0 and bytes > 0,
		"precache did not retain head models, TRI morphs, and bytes")
	check(not player.call("is_menu_open"), "precache opened a menu before character generation")
	check(not player.call("debug_invoke_chargen_native", "InvalidName"), "unknown cache native was accepted")
	check(int(streamer.call("chargen_resident_bytes")) == bytes, "unknown native changed cache accounting")
	check(player.call("debug_invoke_chargen_native", "PrecacheCharGenClear"), "clear native did not run")
	for method in ["chargen_resident_count", "chargen_model_count", "chargen_morph_count", "chargen_missing_count", "chargen_resident_bytes"]:
		check(int(streamer.call(method)) == 0, "%s remained nonzero after clear" % method)
	check(player.call("debug_invoke_chargen_native", "PrecacheCharGenClear"), "repeated clear did not run")
	check(player.call("debug_invoke_chargen_native", "PrecacheCharGen"), "repeated precache did not run")
	check(int(streamer.call("chargen_resident_bytes")) == bytes, "repeated precache changed byte accounting")
	player.call("debug_invoke_chargen_native", "PrecacheCharGenClear")
	check(int(streamer.call("chargen_resident_bytes")) == 0, "final clear retained cache bytes")
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " UI-17 chargen residency")
	quit(0 if failures == 0 else 1)
"#,
    )
    .unwrap();

    let output = Command::new(godot)
        .current_dir(root)
        .env("SKYRIM_DATA", data)
        .args(["--headless", "--path", "godot", "--script"])
        .arg(script)
        .output()
        .unwrap();
    assert_godot_passed(&output, "PASS UI-17 chargen residency");
}
