extends SceneTree

## Save a live player and attach the verified state to a fresh host player.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

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

func _run() -> void:
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "installed data did not load")
	var original := _player(streamer)
	_check(original.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"source cell did not load")
	_check(original.call("start_gameplay"), "source Game did not start")
	original.call("set_facing", 1.2, -0.1)
	var directory := OS.get_environment("SAV_19_SAVE_DIR")
	var saved := original.call("save_game_generation", directory, 2)
	_check(saved.get("ok", false), "source native generation did not save")
	var expected_cell: String = original.call("gameplay_cell")
	var expected_position: Vector3 = original.call("position")
	var expected_facing: Vector2 = original.call("facing")
	var destination := _player(streamer)
	var rejected := destination.call("attach_restored_generation", directory + "/missing", 2)
	_check(not rejected.get("ok", true), "missing generation was accepted")
	_check(destination.call("gameplay_cell").is_empty(), "failed import changed the host cell")
	var before_days: float = destination.call("gameplay_days")
	var attached := destination.call("attach_restored_generation", directory, 2)
	_check(attached.get("ok", false) and attached.get("state_loaded", false),
		"verified generation did not attach")
	_check(destination.call("gameplay_cell") == expected_cell, "restored player cell was not attached")
	var position: Vector3 = destination.call("position")
	_check(position.is_finite(), "restored host pose is not finite")
	_check(position.distance_to(expected_position) < 5.0, "restored player pose was not installed")
	_check(destination.call("facing").distance_to(expected_facing) < 0.01,
		"restored camera facing was not installed")
	var camera := destination.get_node_or_null("Camera3D") as Camera3D
	_check(camera != null and camera.is_current(), "restored camera was not made current")
	_check(destination.call("gameplay_days") > before_days, "attach did not advance a Game frame")
	await process_frame
	destination.free()
	original.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " sav_19 host attach")
	quit(0 if failures == 0 else 1)
