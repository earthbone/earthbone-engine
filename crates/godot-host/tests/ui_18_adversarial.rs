// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial host checks for UI-18 title animation registration and playback.

use std::process::{Command, Output};

fn workspace() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn godot() -> std::ffi::OsString {
    std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must point to Godot 4.4")
}

fn assert_godot_passed(output: Output, marker: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains(marker), "{stdout}\n{stderr}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}

#[test]
fn ui_18_title_sequence_plays_and_quit_clears_it_in_the_live_host() {
    let root = workspace();
    let output = Command::new(godot())
        .current_dir(&root)
        .args([
            "--windowed",
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "ui_18_main_menu_test.gd",
        ])
        .output()
        .expect("Godot host check could not start");
    assert_godot_passed(output, "PASS UI-18 main menu lifecycle");
}

#[test]
fn ui_18_registration_handles_empty_names_and_repeated_registration() {
    let root = workspace();
    let temp = tempfile::tempdir().expect("temporary directory");
    let script = temp.path().join("ui_18_registration_breaker.gd");
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
	var player: Node = ClassDB.instantiate("PlayerNode")
	root.add_child(player)
	var original := Animation.new()
	original.set_length(0.25)
	check(not player.call("register_title_sequence", "", original), "empty sequence name was accepted")
	check(player.call("register_title_sequence", "Intro", original), "valid sequence was rejected")
	var replacement := Animation.new()
	replacement.set_length(0.75)
	check(player.call("register_title_sequence", "Intro", replacement), "re-registering a name was rejected")
	var animation_player: AnimationPlayer = player.get_node("TitleSequencePlayer")
	check(is_equal_approx(animation_player.get_animation("Intro").length, 0.75), "re-registered animation is not reachable at its name")
	animation_player.play("Intro")
	check(animation_player.is_playing(), "registered sequence is not playing")
	for frame in 3:
		await process_frame
	check(animation_player.get_current_animation_position() > 0.0, "registered sequence playhead did not advance")
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " UI-18 registration breaker")
	quit(0 if failures == 0 else 1)
"#,
    )
    .expect("write temporary Godot check");

    let output = Command::new(godot())
        .current_dir(&root)
        .args(["--headless", "--path", "godot", "--script"])
        .arg(script)
        .output()
        .expect("Godot registration check could not start");
    assert_godot_passed(output, "PASS UI-18 registration breaker");
}
