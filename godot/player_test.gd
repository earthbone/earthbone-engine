extends SceneTree

## Runs PlayerNode inside Godot. No data is needed for input/camera checks.
## SKYRIM_DATA also exercises collision and movement in the Bannered Mare.

const STEP := 1.0 / 60.0
var player: Node3D
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _key(code: Key, down: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = code
	event.pressed = down
	Input.parse_input_event(event)
	Input.flush_buffered_events()

func _step() -> void:
	player.call("advance", STEP)

func _run() -> void:
	if not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: PlayerNode is not registered")
		quit(1)
		return
	player = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.is_node_ready(), "player did not enter the tree")
	_check(InputMap.has_action("Forward"), "ready did not publish bindings")
	_check(not player.call("is_third_person"), "initial camera is not first person")
	Input.action_press("Toggle POV")
	_step()
	_check(player.call("is_third_person"), "POV action did not toggle")
	_step()
	_check(player.call("is_third_person"), "held POV toggled again in one frame")
	Input.action_release("Toggle POV")
	_step()

	# Loading a map after ready must replace the registered keyboard event.
	player.call("apply_control_map", "Toggle POV\t0x22\t0xff\t0xff\t1\t1\t1\n")
	var events := InputMap.action_get_events("Toggle POV")
	_check(events.size() == 1 and events[0].keycode == KEY_G,
		"late control-map update left the old InputMap binding")
	_key(KEY_F, true)
	_step()
	_check(player.call("is_third_person"), "old POV key still acts after rebinding")
	_key(KEY_F, false)
	await process_frame
	_step()
	_key(KEY_G, true)
	_step()
	_check(not player.call("is_third_person"), "replacement POV key did not act")
	_key(KEY_G, false)
	await process_frame
	_step()

	player.call("apply_control_map", "Toggle POV\t0x2a+0x22\t0xff\t0xff\t1\t1\t1\n")
	_key(KEY_G, true)
	_step()
	_check(not player.call("is_third_person"), "a chord fired without its modifier")
	_key(KEY_SHIFT, true)
	_step()
	_check(player.call("is_third_person"), "completing the chord did not fire")
	_step()
	_check(player.call("is_third_person"), "held chord fired twice")
	_key(KEY_SHIFT, false)
	_key(KEY_G, false)
	await process_frame
	_step()
	player.call("apply_control_map", "")
	Input.action_press("Toggle POV")
	Input.action_release("Toggle POV")
	_step()
	_check(not player.call("is_third_person"), "a tap between ticks was lost")
	_step()
	_check(not player.call("is_third_person"), "a short tap was consumed twice")
	_key(KEY_W, true)
	_step()
	player.call("apply_control_map", "Forward\t0x23\t0xff\t0xff\t1\t1\t1\n")
	_check(not Input.is_action_pressed("Forward"), "rebinding left the old action held")
	_key(KEY_W, false)
	await process_frame
	_step()
	player.call("apply_control_map", "")

	var before: Vector2 = player.call("facing")
	var motion := InputEventMouseMotion.new()
	motion.relative = Vector2(20, -10)
	Input.parse_input_event(motion)
	Input.flush_buffered_events()
	_step()
	var after: Vector2 = player.call("facing")
	_check(after.x > before.x and after.y > before.y, "mouse look has wrong direction")
	player.call("set_facing", 0.0, 0.0)

	var data := OS.get_environment("SKYRIM_DATA")
	if not data.is_empty():
		if player.call("load_data", data):
			var bodies: int = player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0)
			_check(bodies > 0, "cell supplied no collision bodies")
			if bodies > 0:
				for i in range(600):
					_step()
				_check(player.call("is_on_ground"), "player never settled on cell collision")
				var start: Vector3 = player.call("position")
				Input.action_press("Forward")
				for i in range(30):
					_step()
				Input.action_release("Forward")
				_step()
				var end: Vector3 = player.call("position")
				_check(start.distance_to(end) > 1.0, "movement action did not move the player")
				print("PLAYER: bodies=", bodies, " start=", start, " end=", end)
		else:
			_check(false, "load_data failed")
	else:
		print("SKIP: real-cell movement, SKYRIM_DATA unset")
	player.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " player runtime")
	quit(0 if failures == 0 else 1)
