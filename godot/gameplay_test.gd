extends SceneTree

## Installed-data integration: one shared load, real VM, exterior door and return.
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
		printerr("FAIL: gameplay_test requires SKYRIM_DATA and the extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	_check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0, "initial render failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared source failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"initial collision failed")
	var requested: String = player.call("gameplay_cell")
	if not player.call("start_gameplay"):
		_check(false, "runtime start failed")
	else:
		var start: String = player.call("gameplay_cell")
		_check(start == requested, "startup quests replaced the requested walkthrough cell")
		var doors: Array = player.call("gameplay_doors")
		var chosen: Dictionary = {}
		for door: Dictionary in doors:
			if not door.locked:
				chosen = door
				break
		_check(not chosen.is_empty(), "no unlocked load door")
		if not chosen.is_empty():
			_check(player.call("activate_reference", chosen.file, chosen.object), "outbound activation failed")
			var outside: String = player.call("gameplay_cell")
			_check(outside != start, "player did not leave the interior")
			_check(streamer.call("instance_count") > 0, "exterior has no draw instances")
			var landing: Vector3 = player.call("position")
			for i in range(120):
				player.call("advance", 1.0 / 60.0)
			_check(player.call("is_on_ground"), "exterior landing has no supporting collision")
			var grounded: Vector3 = player.call("position")
			_check(absf(grounded.z - landing.z) < 256.0, "player fell away from exterior landing")
			_check(player.call("activate_reference", chosen.far_file, chosen.far_object), "return activation failed")
			_check(player.call("gameplay_cell") == start, "return did not restore interior identity")
			_check(streamer.call("instance_count") > 0, "returned interior has no draw instances")
			for i in range(120):
				player.call("advance", 1.0 / 60.0)
			_check(player.call("is_on_ground"), "interior landing has no supporting collision")
			print("GAMEPLAY ROUNDTRIP: ", start, " -> ", outside, " -> ", player.call("gameplay_cell"),
				" landing=", landing, " grounded=", grounded)
			# Teleport markers face into the destination. Turn back toward the door.
			var facing: Vector2 = player.call("facing")
			player.call("set_facing", facing.x + PI, 0.0)
			print("GAMEPLAY AIM: feet=", player.call("position"), " original=", facing,
				" facing=", player.call("facing"))
			var key := InputEventKey.new()
			key.keycode = KEY_E
			key.pressed = true
			Input.parse_input_event(key)
			Input.flush_buffered_events()
			player.call("advance", 1.0 / 60.0)
			key = InputEventKey.new()
			key.keycode = KEY_E
			key.pressed = false
			Input.parse_input_event(key)
			Input.flush_buffered_events()
			var picked: String = player.call("last_activate_reference")
			var known_door := false
			for door: Dictionary in doors:
				known_door = known_door or picked == "%d:%06X" % [door.file, door.object]
			_check(known_door, "Activate ray did not identify a load door: " + picked)
			_check(player.call("gameplay_cell") == outside, "Activate input did not cross the door")
			print("GAMEPLAY INPUT: hit=", player.call("last_activate_hit"), " picked=", picked,
				" facing=", player.call("facing"), " destination=", player.call("gameplay_cell"))
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " gameplay runtime")
	quit(0 if failures == 0 else 1)
