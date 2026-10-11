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
fn ui_18_breaker3_quit_closes_live_menu_and_title_sequence_animates() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Installed data and a Godot binary are both optional: skip without either.
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("ui_18_breaker3.gd");
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
	check(player.call("use_streamer_data", streamer), "player did not share game records")
	check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"gameplay cell did not load")
	check(player.call("start_gameplay"), "game runtime did not start")
	check(player.call("open_menu", "Pause"), "modal menu did not open")
	check(player.call("debug_invoke_main_menu_native", "QuitToMainMenu", ""), "quit native did not run")
	check(player.call("is_main_menu"), "quit did not select the main menu")
	check(not player.call("is_menu_open"), "quit retained an open gameplay menu")
	check(player.call("gameplay_cell").is_empty(), "quit retained the gameplay cell")
	check(not player.call("debug_invoke_main_menu_native", "QuitToMainMenu", ""),
		"quit retained the old Game runtime")
	check(player.call("use_streamer_data", streamer), "player could not reattach game records")
	check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"gameplay cell did not reload")
	check(player.call("start_gameplay"), "game runtime did not restart")
	var animation := Animation.new()
	animation.set_length(2.0)
	var probe := Node3D.new()
	probe.name = "TitleSequenceProbe"
	player.add_child(probe)
	var track := animation.add_track(Animation.TYPE_VALUE)
	animation.track_set_path(track, NodePath("TitleSequenceProbe:position"))
	animation.track_insert_key(track, 0.0, Vector3.ZERO)
	animation.track_insert_key(track, 2.0, Vector3(0.0, 20.0, 0.0))
	check(player.call("register_title_sequence", "MQ101Intro", animation), "title animation did not register")
	player.get_node("TitleSequencePlayer").root_node = NodePath("..")
	var returned: Array[bool] = []
	var started: Array[String] = []
	player.returned_to_main_menu.connect(func(): returned.append(true))
	player.title_sequence_started.connect(func(name: String): started.append(name))
	check(player.call("debug_invoke_main_menu_native", "StartTitleSequence", "MQ101Intro"),
		"title native did not run")
	check(player.call("is_main_menu"), "title sequence did not enter the main menu")
	check(player.call("active_title_sequence") == "MQ101Intro", "host lost the requested sequence name")
	check(player.call("is_title_sequence_playing"), "AnimationPlayer did not start the sequence")
	check(started == ["MQ101Intro"], "host did not signal the played sequence")
	check(returned.size() == 1, "title sequence did not signal its main menu transition")
	for frame in 4:
		await process_frame
	check(float(player.call("title_sequence_playback_position")) > 0.0, "title animation playhead did not advance")
	check(probe.position.y > 0.0, "title animation did not animate its target")
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " UI-18 teardown and playback")
	quit(0 if failures == 0 else 1)
"#,
    )
    .unwrap();

    let output = Command::new(godot)
        .current_dir(root)
        .env("SKYRIM_DATA", data)
        .args([
            "--windowed",
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
        ])
        .arg(script)
        .output()
        .unwrap();
    assert_godot_passed(&output, "PASS UI-18 teardown and playback");
}
