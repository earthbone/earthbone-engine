extends SceneTree

## Player movement in the Bannered Mare must reach the audio mixer.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _key(code: Key, held: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = code
	event.pressed = held
	Input.parse_input_event(event)
	Input.flush_buffered_events()

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: installed data and PlayerNode required")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared source failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", -125.0, -800.0, 120.0) > 0, "cell collision failed")
	_check(player.call("start_gameplay"), "gameplay start failed")
	for i in range(120):
		player.call("advance", 1.0 / 60.0)
	_check(player.call("is_on_ground"), "initial player did not land")
	_check(player.call("footstep_count") == 0, "standing still produced steps")
	var capture := AudioEffectCapture.new()
	capture.buffer_length = 4.0
	AudioServer.add_bus_effect(0, capture)
	var before: Vector3 = player.call("position")
	player.call("set_facing", 0.0, 0.0)
	_key(KEY_W, true)
	for i in range(120):
		player.call("advance", 1.0 / 60.0)
		await create_timer(1.0 / 60.0).timeout
	_key(KEY_W, false)
	var after: Vector3 = player.call("position")
	var steps: int = player.call("footstep_count")
	_check(steps >= 1, "walking produced no footsteps")
	_check(player.call("footstep_errors") == 0, "footstep selection or playback failed")
	var samples := capture.get_buffer(capture.get_frames_available())
	var peak := 0.0
	for sample: Vector2 in samples:
		peak = maxf(peak, maxf(absf(sample.x), absf(sample.y)))
	_check(peak > 0.0001, "walking mixer output is silent")
	for i in range(120):
		player.call("advance", 1.0 / 60.0)
	_check(player.call("footstep_count") == steps, "stationary player kept producing steps")
	print("FOOTSTEPS: from=", before, " to=", after, " steps=", steps,
		" frames=", samples.size(), " peak=", peak, " errors=", player.call("footstep_errors"))
	AudioServer.remove_bus_effect(0, AudioServer.get_bus_effect_count(0) - 1)
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " player footsteps")
	quit(0 if failures == 0 else 1)
