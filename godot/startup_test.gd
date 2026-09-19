extends SceneTree

## Exercises asynchronous data loading and the shared player source.
var failures := 0

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	if not ClassDB.class_exists("CellStreamerNode") or not ClassDB.class_exists("PlayerNode"):
		printerr("Host extension is not loaded")
		quit(1)
		return
	var streamer = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	var player = ClassDB.instantiate("PlayerNode")
	player.capture_mouse = false
	player.auto_step = false
	get_root().add_child(player)
	_check(streamer.poll_load_data() == -1, "unrequested source reports ready")
	_check(not player.use_streamer_data(streamer), "unloaded source can be shared")
	_check(streamer.begin_load_data("user://missing-startup-fixture"), "could not start failure test")
	_check(not streamer.begin_load_data("user://another-request"), "pending load was replaced")
	var state: int = streamer.poll_load_data()
	var deadline := Time.get_ticks_msec() + 60000
	while state == 0 and Time.get_ticks_msec() < deadline:
		await process_frame
		state = streamer.poll_load_data()
	_check(state == -1, "missing data did not fail")
	_check(not player.use_streamer_data(streamer), "failed source can be shared")
	var data := OS.get_environment("SKYRIM_DATA")
	if not data.is_empty():
		_check(streamer.begin_load_data(data), "cannot retry after a failed load")
		state = streamer.poll_load_data()
		var frames := 0
		var started := Time.get_ticks_msec()
		deadline = started + 60000
		while state == 0 and Time.get_ticks_msec() < deadline:
			await process_frame
			frames += 1
			state = streamer.poll_load_data()
		_check(state == 1, "installed data did not load")
		_check(frames > 0, "loading never yielded a frame")
		_check(player.use_streamer_data(streamer), "ready source was not shared")
		var events := InputMap.action_get_events("Forward")
		_check(not events.is_empty() and events[0].keycode == KEY_W, "shared load did not publish PC bindings")
		print("STARTUP TEST: data ms=", Time.get_ticks_msec() - started, " responsive frames=", frames)
		streamer.free()
		_check(player.enter_cell("WhiterunBanneredMare", 0, 0, 500) > 0, "source died with streamer")
	else:
		streamer.free()
	player.free()
	var closing = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(closing)
	_check(closing.begin_load_data(data if not data.is_empty() else "user://missing-shutdown-fixture"), "cannot start shutdown load")
	# Free before collecting the worker result to exercise join-on-drop.
	closing.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " startup runtime")
	quit(0 if failures == 0 else 1)
