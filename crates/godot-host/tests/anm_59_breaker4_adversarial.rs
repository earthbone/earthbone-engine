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
fn anm_59_breaker4_bound_head_holds_override_and_resumes_latest_mood() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data =
        std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA must name installed Skyrim data");
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must name Godot 4.4");
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("anm_59_breaker4.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree

const HEAD := "meshes/actors/character/character assets/malehead.nif"

var failures := 0

func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func weight(weights: Dictionary, name: String) -> float:
	return float(weights.get(name, -1.0))

func _initialize() -> void:
	call_deferred("run_checks")

func run_checks() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	check(streamer.call("load_data", data), "vanilla data did not load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	check(player.call("use_streamer_data", streamer), "player did not share the world")
	check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"gameplay cell did not load")
	check(player.call("start_gameplay"), "game runtime did not start")
	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	root.add_child(visual)
	check(visual.call("load_character", data, HEAD), "vanilla head did not load")
	check(player.call("bind_actor_expression_head", visual, 0, 0x14), "player actor did not bind")
	var stats: Dictionary = visual.call("stats")
	check(int(stats.get("expression_shapes", 0)) > 0, "head has no expression morph targets")
	check(player.call("debug_set_player_expression_mood", 3.0), "record mood could not be set to Happy")
	visual.call("advance", 1.0 / 60.0)
	var happy: Dictionary = visual.call("debug_expression_weights")
	check(is_equal_approx(weight(happy, "MoodHappy"), 1.0), "record mood did not apply full MoodHappy")
	check(player.call("debug_set_player_expression_override", 12, 45), "45 percent override failed")
	check(not player.call("debug_set_player_expression_override", 17, 50), "invalid mood was accepted")
	check(player.call("debug_set_player_expression_mood", 4.0), "record mood could not change while overridden")
	visual.call("advance", 1.0 / 60.0)
	var held: Dictionary = visual.call("debug_expression_weights")
	check(is_equal_approx(weight(held, "MoodSurprise"), 0.45), "override strength was not applied to head morph")
	check(is_zero_approx(weight(held, "MoodHappy")), "mood replaced an active override")
	check(player.call("debug_clear_player_expression_override"), "override did not clear")
	visual.call("advance", 1.0 / 60.0)
	var resumed: Dictionary = visual.call("debug_expression_weights")
	check(is_equal_approx(weight(resumed, "MoodSad"), 1.0), "latest record mood did not resume after clear")
	check(is_zero_approx(weight(resumed, "MoodHappy")), "old record mood remained after clear")
	check(is_zero_approx(weight(resumed, "MoodSurprise")), "cleared override remained on the head")
	check(player.call("debug_clear_player_expression_override"), "repeated clear failed")
	visual.free()
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ANM-59 latest mood")
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
    assert_godot_passed(&output, "PASS ANM-59 latest mood");
}
