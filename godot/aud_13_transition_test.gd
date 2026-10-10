extends SceneTree

## AUD-13 measures every half-second mixer window after each load-door crossing.
const FLOOR_DB := -60.0
const WINDOW_MS := 500
const DURATION_MS := 20000
var failures := 0

func _initialize() -> void:
	create_timer(180.0).timeout.connect(func():
		printerr("FAIL: AUD-13 scene exceeded 180 seconds")
		quit(1))
	call_deferred("aud_13_bannered_mare_to_plains_continuous_audio")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		if failures <= 10:
			printerr("FAIL: ", message)

func _wait_music(bank: Node) -> void:
	var started := Time.get_ticks_msec()
	while int(bank.call("music_playing_count")) == 0 and Time.get_ticks_msec() - started < 15000:
		await process_frame
	_check(int(bank.call("music_playing_count")) > 0, "record music decoded: " + str(bank.call("last_error")))

func _sample(player: Node, bank: Node, capture: AudioEffectCapture, walk: bool) -> void:
	capture.clear_buffer()
	var started := Time.get_ticks_msec()
	var last_motion := started
	var peak := 0.0
	var minimum_db := INF
	var windows := 0
	var window_frames := 0
	var frames_per_window := int(AudioServer.get_mix_rate() * float(WINDOW_MS) / 1000.0)
	var cells := {}
	var start_position: Vector3 = player.call("position")
	var facing: Vector2 = player.call("facing")
	var walk_seconds := 0.0
	while windows < DURATION_MS / WINDOW_MS and Time.get_ticks_msec() - started < 60000:
		await process_frame
		if walk:
			var now := Time.get_ticks_msec()
			var seconds := float(now - last_motion) / 1000.0
			last_motion = now
			walk_seconds += seconds
			var direction := Vector3(sin(facing.x), cos(facing.x), 0) if walk_seconds < 5.0 else Vector3(0, -1, 0)
			var steps := maxi(1, int(ceil(seconds * 60.0)))
			for _step in range(steps):
				_check(int(player.call("sample_detection_walk", maxf(seconds / steps, 0.001), direction)) >= 0, "host walking and streaming")
		cells[player.call("gameplay_cell")] = true
		var frames := capture.get_frames_available()
		if frames > 0:
			for sample: Vector2 in capture.get_buffer(frames):
				if windows == DURATION_MS / WINDOW_MS:
					break
				peak = maxf(peak, maxf(absf(sample.x), absf(sample.y)))
				window_frames += 1
				if window_frames == frames_per_window:
					var db := -INF if peak <= 0.0 else 20.0 * log(peak) / log(10.0)
					minimum_db = minf(minimum_db, db)
					_check(db > FLOOR_DB, "master window %d at %.2f dB" % [windows, db])
					peak = 0.0
					window_frames = 0
					windows += 1
	var distance := start_position.distance_to(player.call("position"))
	_check(windows == DURATION_MS / WINDOW_MS, "twenty seconds of mixer samples")
	_check(is_instance_valid(bank) and bank.get_parent() == player, "player owns surviving audio root")
	if walk:
		_check(distance > 100.0, "walked on plains")
		_check(cells.size() > 1, "walk crossed exterior CELL boundary")
	print("AUD-13 mixer seconds=", windows * float(WINDOW_MS) / 1000.0,
		" windows=", windows, " minimum_peak_db=", minimum_db, " cells=", cells,
		" position=", player.call("position"), " facing=", facing,
		" distance=", distance, " music=", bank.call("music_start_count"),
		" ambience=", bank.call("ambience_start_count"))

func aud_13_bannered_mare_to_plains_continuous_audio() -> void:
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "installed data")
	_check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0, "Bannered Mare render")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared source")
	_check(player.call("enter_cell", "WhiterunBanneredMare", -125.0, -800.0, 120.0) > 0, "interior entry")
	_check(player.call("start_gameplay"), "gameplay starts")
	var bank := player.get_node("SoundBank")
	var bank_id := bank.get_instance_id()
	await _wait_music(bank)
	await create_timer(1.2).timeout
	_check(not str(bank.call("acoustic_space")).is_empty(), "interior acoustic space")
	_check(int(bank.call("ambience_playing_count")) > 0, "interior ambience")
	var capture := AudioEffectCapture.new()
	capture.buffer_length = 30.0
	AudioServer.add_bus_effect(0, capture)
	var left_interior := false
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			_check(player.call("activate_reference", door.file, door.object), "Bannered Mare exit door")
			left_interior = true
			break
	_check(left_interior, "found Bannered Mare exit")
	_check(player.get_node("SoundBank").get_instance_id() == bank_id, "door retains SoundBank")
	player.call("advance", 0.0)
	_check(player.call("detection_worldspace") == "WhiterunWorld", "city exterior reached")
	await _sample(player, bank, capture, false)
	await _wait_music(bank)
	var starts := int(bank.call("music_start_count"))
	for slot in range(2):
		var music := bank.get_node("MusicPlayer" + str(slot)) as AudioStreamPlayer
		if music.playing:
			music.seek(maxf(0.0, music.stream.get_length() - 0.1))
	var end_wait := Time.get_ticks_msec()
	while int(bank.call("music_start_count")) == starts and Time.get_ticks_msec() - end_wait < 15000:
		await process_frame
	_check(int(bank.call("music_start_count")) > starts, "finished track selects and starts another")
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "Whiterun gate destination")
	if gate.is_empty():
		quit(1)
		return
	_check(player.call("queue_player_input", {"action": "move_to", "file": gate.file, "object": gate.object,
		"offset": Vector3(0, 200, 0), "match_rotation": true}), "walk route to city gate")
	_check(player.call("step_player_input"), "reach city gate")
	player.call("advance", 0.0)
	_check(player.call("activate_reference", gate.file, gate.object), "Whiterun gate crossing")
	player.call("advance", 0.0)
	_check(player.call("detection_worldspace") == "Tamriel", "plains worldspace reached")
	_check(player.get_node("SoundBank").get_instance_id() == bank_id, "worldspace transition retains audio root")
	await _sample(player, bank, capture, true)
	player.free()
	streamer.free()
	print("AUD-13 RESULT: ", "PASS" if failures == 0 else "FAIL", " Bannered Mare to plains")
	quit(0 if failures == 0 else 1)
