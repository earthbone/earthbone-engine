// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! AI-60 checks for the exported host animation state feed.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn ai_60_host_reports_validate_context_values_and_rig_lifetime() {
    let godot = std::env::var_os("SOE_GODOT_BIN")
        .map(PathBuf::from)
        .unwrap();
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from).unwrap();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    assert!(target.join("release/libsoe_godot_host.so").is_file());

    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let script_dir = tempfile::tempdir().unwrap();
    let script = script_dir.path().join("ai_60_host_magic_feed.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree

const BODY := "meshes/actors/character/character assets/malebody_1.nif"
const PLAYER := 0x14
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	if not GDExtensionManager.is_extension_loaded("res://soe.gdextension"):
		GDExtensionManager.load_extension("res://soe.gdextension")
	var unbound: Node3D = ClassDB.instantiate("SkinnedActorNode")
	_check(unbound != null, "SkinnedActorNode is registered")
	if unbound != null:
		_check(not unbound.call("report_ward_state", 0), "unbound ward report rejected")
		_check(not unbound.call("report_dual_casting", 1), "unbound dual-cast report rejected")
		_check(not unbound.call("report_shout_variation", 2), "unbound shout report rejected")
		unbound.free()

	var data := OS.get_environment("SKYRIM_DATA")
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "installed data load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"player cell load")
	_check(player.call("start_gameplay"), "gameplay start")
	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	get_root().add_child(visual)
	_check(visual.call("load_character", data, BODY), "vanilla actor graph load")
	_check(player.call("bind_actor_appearance", visual, 0, PLAYER), "player rig binding")

	if failures == 0:
		for state in [0, 1, 2, 0, -1, 2]:
			_check(visual.call("report_ward_state", state), "valid ward report %d" % state)
		for state in [-2, 3, 2147483647]:
			_check(not visual.call("report_ward_state", state), "invalid ward report %d" % state)
		for state in [1, 0, 1, -1, 0]:
			_check(visual.call("report_dual_casting", state), "valid dual-cast report %d" % state)
		for state in [-2, 2, 2147483647]:
			_check(not visual.call("report_dual_casting", state), "invalid dual-cast report %d" % state)
		for state in [2, 1, 0, -1, 2]:
			_check(visual.call("report_shout_variation", state), "valid shout report %d" % state)
		for state in [-2, 3, 2147483647]:
			_check(not visual.call("report_shout_variation", state), "invalid shout report %d" % state)
		visual.free()
		await process_frame

	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ai_60 host magic feed")
	quit(0 if failures == 0 else 1)
"#,
    )
    .unwrap();

    let output = Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project)
        .args(["--script"])
        .arg(script)
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS ai_60 host magic feed"),
        "AI-60 Godot host feed failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
