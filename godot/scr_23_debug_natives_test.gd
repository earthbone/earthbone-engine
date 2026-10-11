extends SceneTree

## Exercise reference markers and latent player movement in the live host.
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
		printerr("FAIL: scr_23 requires SKYRIM_DATA and the extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	_check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0, "cell render failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"player collision failed")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	var named_references: Array = player.call("gameplay_named_references")
	_check(not named_references.is_empty(), "Bannered Mare has no named placed reference")
	if not named_references.is_empty():
		var target: Dictionary = named_references[0]
		var editor_id: String = target.editor_id
		_check(player.call("scr_23_invoke_debug_native", "ShowRefPosition", "", target.file, target.object),
			"ShowRefPosition native failed")
		player.call("advance", 1.0 / 60.0)
		_check(player.call("scr_23_debug_marker_count") == 1,
			"ShowRefPosition did not install one RenderingServer marker")
		_check(player.call("scr_23_invoke_debug_native", "PlayerMoveToAndWait", editor_id, 0, 0),
			"PlayerMoveToAndWait native failed")
		player.call("advance", 1.0 / 60.0)
		var destination: Dictionary = player.call("gameplay_reference_pose", target.file, target.object)
		var player_pose: Dictionary = player.call("gameplay_reference_pose", 0, 20)
		_check(destination.has("position") and player_pose.has("position"), "moved references have no placement")
		if destination.has("position") and player_pose.has("position"):
			_check(destination.position.is_equal_approx(player_pose.position),
				"latent host move did not install the destination placement")
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " scr_23 live host")
	quit(0 if failures == 0 else 1)
