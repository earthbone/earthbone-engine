extends SceneTree

## MAG-17 installed-data test for live camera selection and target-location casting.
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
		printerr("FAIL: MAG-17 requires SKYRIM_DATA and the host extension, data=",
			data.is_empty(), " PlayerNode=", ClassDB.class_exists("PlayerNode"))
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "installed data load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", -125.0, -800.0, 120.0) > 0,
		"Bannered Mare collision load failed")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	mag_17_live_host_selection_casts_at_selected_position(player)
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " MAG-17 live location selection")
	quit(0 if failures == 0 else 1)

func mag_17_live_host_selection_casts_at_selected_position(player: Node3D) -> void:
	var selected := PackedVector3Array()
	var cast := false
	for index in range(16):
		player.call("set_facing", TAU * float(index) / 16.0, -1.39)
		selected = player.call("selected_location_position")
		if selected.size() == 1:
			var point: Vector3 = selected[0]
			if point.is_finite() and player.call("cast_location_spell_at_look", 0, 0x5DB90):
				cast = true
				break
	_check(selected.size() == 1, "camera ray did not select loaded static geometry")
	_check(cast, "Fire Rune did not cast at its live selected world point")
