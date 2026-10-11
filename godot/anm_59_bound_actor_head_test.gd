extends SceneTree

const PLAYER_ACTOR := 0x14
const HEAD := "meshes/actors/character/character assets/malehead.nif"
const CELL := "WhiterunBanneredMare"

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: ANM-59 requires SKYRIM_DATA and the host extension")
		quit(1)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "vanilla data load failed")
	_check(streamer.call("stream_cell", CELL) > 0, "vanilla test cell did not stream")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source failed")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0,
		"vanilla test cell did not enter")
	_check(player.call("start_gameplay"), "gameplay startup failed")

	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	get_root().add_child(visual)
	_check(visual.call("load_character", data, HEAD), "vanilla actor head failed to load")
	_check(player.call("bind_actor_expression_head", visual, 0, PLAYER_ACTOR),
		"vanilla head did not bind to the player's expression state")
	var stats: Dictionary = visual.call("stats")
	_check(int(stats.get("expression_shapes", 0)) > 0,
		"bound vanilla actor head has no expression blend shapes")
	_check(player.call("debug_set_player_expression_mood", 3.0),
		"player record mood could not be set to Happy")

	visual.call("advance", 1.0 / 60.0)
	var baseline: Dictionary = visual.call("debug_expression_weights")
	_check(is_equal_approx(float(baseline.get("MoodHappy", -1.0)), 1.0),
		"record mood did not select the full strength MoodHappy target")
	_check(player.call("debug_set_player_expression_override", 12, 45),
		"player expression override could not be set")
	visual.call("advance", 1.0 / 60.0)
	var overridden: Dictionary = visual.call("debug_expression_weights")
	_check(is_equal_approx(float(overridden.get("MoodSurprise", -1.0)), 0.45),
		"bound head did not receive MoodSurprise at 45 percent")
	for target in overridden:
		if target != "MoodSurprise":
			_check(is_zero_approx(float(overridden[target])),
				"competing expression %s remained active" % target)

	_check(player.call("debug_clear_player_expression_override"),
		"player expression override could not be cleared")
	visual.call("advance", 1.0 / 60.0)
	var resumed: Dictionary = visual.call("debug_expression_weights")
	_check(resumed == baseline, "bound head did not resume its record mood after clearing")

	visual.free()
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ANM-59 bound actor head")
	quit(0 if failures == 0 else 1)
