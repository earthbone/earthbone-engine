extends SceneTree

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _wait_for_music_count(bank: Node, expected: int) -> bool:
	for _frame in range(600):
		if int(bank.call("music_start_count")) >= expected:
			return true
		await process_frame
	return false

func _run() -> void:
	var extension := OS.get_environment("AUD_05_EXTENSION")
	if not ClassDB.class_exists("PlayerNode"):
		var status := GDExtensionManager.load_extension(extension)
		_check(status == GDExtensionManager.LOAD_STATUS_OK, "host extension loads")
	if not ClassDB.class_exists("PlayerNode") or not ClassDB.class_exists("CellStreamerNode"):
		printerr("FAIL: PlayerNode or CellStreamerNode is unavailable")
		quit(1)
		return
	var data := OS.get_environment("SKYRIM_DATA")
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "installed data opens")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "player shares installed data")
	_check(player.call("enter_cell", "WhiterunBanneredMare", -125.0, -800.0, 120.0) > 0,
		"player enters the test cell")
	_check(player.call("start_gameplay"), "gameplay starts")
	if failures > 0:
		quit(1)
		return
	var bank := player.get_node("SoundBank")
	var first_player := bank.get_child(16) as AudioStreamPlayer
	var second_player := bank.get_child(17) as AudioStreamPlayer
	var first_stream_before := first_player.stream
	var second_stream_before := second_player.stream
	var capture := AudioEffectCapture.new()
	capture.buffer_length = 1.0
	AudioServer.add_bus_effect(0, capture)
	var baseline := int(bank.call("music_start_count"))
	var selected: bool = player.call("select_music_type",
		int(OS.get_environment("AUD_05_MUSIC_FILE")),
		int(OS.get_environment("AUD_05_MUSIC_OBJECT")),
		float(OS.get_environment("AUD_05_MUSIC_DRAW")))
	_check(selected, "PlayerNode selects an eligible vanilla MUSC")
	_check(await _wait_for_music_count(bank, baseline + 1),
		"record selection starts decoded Godot playback")
	await process_frame
	var selected_player := first_player
	var selected_stream_changed := first_player.stream != first_stream_before
	if first_player.stream == first_stream_before:
		selected_player = second_player
		selected_stream_changed = second_player.stream != second_stream_before
	_check(selected_player.playing, "selected track plays in a Godot AudioStreamPlayer")
	_check(selected_stream_changed, "record selection replaced the prior player stream")
	_check(selected_player.volume_db < -0.01,
		"selected record's configured fade-in changes its Godot player volume")
	await create_timer(0.15).timeout
	var peak := 0.0
	for sample: Vector2 in capture.get_buffer(capture.get_frames_available()):
		peak = maxf(peak, maxf(absf(sample.x), absf(sample.y)))
	_check(peak > 0.0001, "selected vanilla track reaches the Godot mixer")
	print("AUD-05 PlayerNode mixer peak: ", peak)
	AudioServer.remove_bus_effect(0, AudioServer.get_bus_effect_count(0) - 1)
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " PlayerNode record music")
	quit(0 if failures == 0 else 1)
