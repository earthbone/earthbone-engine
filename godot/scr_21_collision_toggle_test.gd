extends SceneTree

## Installed-data host check for Debug.ToggleCollisions on resident bodies.
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
		printerr("FAIL: scr_21_collision_toggle_test requires SKYRIM_DATA and the extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	_check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0, "interior render failed")
	_check(streamer.call("stream_cell_collision", "WhiterunBanneredMare") > 0,
		"interior collision bodies failed to load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"interior collision load failed")
	_check(player.call("start_gameplay"), "runtime start failed")
	var initial: Dictionary = streamer.call("collision_filter_state")
	_check(initial.get("total", 0) > 0, "interior has no resident collision bodies")
	_check(player.call("debug_toggle_gameplay_collisions"), "ToggleCollisions native failed")
	var disabled: Dictionary = streamer.call("collision_filter_state")
	for i in range(120):
		if disabled.get("disabled", 0) == initial.get("total", -1):
			break
		player.call("advance", 1.0 / 60.0)
		disabled = streamer.call("collision_filter_state")
	_check(disabled.get("disabled", 0) == initial.get("total", -1)
		and disabled.get("enabled", -1) == 0 and disabled.get("other", -1) == 0,
		"ToggleCollisions did not disable resident bodies: %s" % disabled)
	_check(player.call("debug_toggle_gameplay_collisions"), "ToggleCollisions restore failed")
	var enabled: Dictionary = streamer.call("collision_filter_state")
	for i in range(120):
		if enabled.get("enabled", 0) == initial.get("total", -1):
			break
		player.call("advance", 1.0 / 60.0)
		enabled = streamer.call("collision_filter_state")
	_check(enabled.get("enabled", 0) == initial.get("total", -1)
		and enabled.get("disabled", -1) == 0 and enabled.get("other", -1) == 0,
		"ToggleCollisions did not restore resident bodies: %s" % enabled)
	print("SCR-21 collision toggle: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
