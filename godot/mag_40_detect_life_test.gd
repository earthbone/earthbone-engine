extends SceneTree

## MAG-40 installed-data check for Detect Life's active authored EFSH visual.
const CELL := "WhiterunBanneredMare"
const DETECT_LIFE := 0x000211EE

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var extension_status := GDExtensionManager.load_extension("res://soe.gdextension")
	if extension_status != GDExtensionManager.LOAD_STATUS_OK \
		and extension_status != GDExtensionManager.LOAD_STATUS_ALREADY_LOADED:
		printerr("FAIL: host extension load failed with status ", extension_status)
		quit(1)
		return
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: MAG-40 requires SKYRIM_DATA and the host extension")
		quit(1)
		return

	var streamer := ClassDB.instantiate("CellStreamerNode") as Node3D
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "installed data load failed")
	_check(streamer.call("stream_cell", CELL) > 0, "Bannered Mare render load failed")
	_check(streamer.call("stream_cell_collision", CELL) > 0,
		"Bannered Mare collision load failed")
	var player := ClassDB.instantiate("PlayerNode") as Node3D
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "player did not share installed data")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 0.0) > 0,
		"player failed to enter Bannered Mare")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	_check(streamer.call("effect_shader_render_count") == 0,
		"effect-shader renderer was active before Detect Life")

	if failures == 0:
		_check(player.call("cast_spell_on_self", 0, DETECT_LIFE),
			"vanilla Detect Life did not cast on the player")
		var observed := false
		for _frame in range(120):
			player.call("advance", 1.0 / 60.0)
			await process_frame
			if int(streamer.call("effect_shader_render_count")) > 0:
				observed = true
				break
		_check(observed,
			"active Detect Life did not submit an authored EFSH visual for a nearby actor")

	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " MAG-40 Detect Life visual")
	quit(0 if failures == 0 else 1)
