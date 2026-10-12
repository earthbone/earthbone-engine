extends SceneTree

## PRF-13 exercises the existing key, camera, overlay and post-draw paths.
const Overlay := preload("res://system_overlay.gd")
var failures := 0
var watchdog_timer: Timer


func _initialize() -> void:
	call_deferred("prf_13_f4_input_presentation_and_session_export")
	call_deferred("arm_watchdog")


func arm_watchdog() -> void:
	watchdog_timer = Timer.new()
	watchdog_timer.one_shot = true
	root.add_child(watchdog_timer)
	watchdog_timer.timeout.connect(func() -> void:
		printerr("PRF-13 FAIL: check did not complete")
		quit(1)
	)
	watchdog_timer.start(120.0)


func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-13 FAIL: ", message)


func key(code: Key, down: bool, echo: bool = false) -> void:
	var event := InputEventKey.new()
	event.keycode = code
	event.pressed = down
	event.echo = echo
	Input.parse_input_event(event)
	Input.flush_buffered_events()


func prf_13_f4_input_presentation_and_session_export() -> void:
	check(DisplayServer.get_name() != "headless", "windowed renderer required")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	get_root().add_child(player)
	player.call("set_entity_profiling", false)
	var overlay := Overlay.new()
	overlay.player = player
	get_root().add_child(overlay)
	key(KEY_F4, true)
	check(player.call("entity_profiling_enabled"), "F4 did not enable capture")
	check(overlay.panel.visible, "F4 did not open overlay")
	key(KEY_F4, true, true)
	check(player.call("entity_profiling_enabled"), "echo toggled capture")
	key(KEY_F4, false)
	check(not player.call("is_third_person"), "unexpected initial camera")
	key(KEY_F, true)
	await physics_frame
	await process_frame
	await RenderingServer.frame_post_draw
	check(player.call("is_third_person"), "key did not apply camera switch")
	key(KEY_F, false)
	# Immediate menu changes must be presented even with physics stepping off.
	player.set("auto_step", false)
	key(KEY_ESCAPE, true)
	await process_frame
	await RenderingServer.frame_post_draw
	var immediate_dir := ProjectSettings.globalize_path("user://")
	check(player.call("write_entity_profile", immediate_dir), "immediate menu export")
	var immediate: Variant = JSON.parse_string(FileAccess.get_file_as_string(immediate_dir.path_join("soe-profile.json")))
	var immediate_found := false
	if immediate is Dictionary:
		for event: Dictionary in immediate.traceEvents:
			if event.cat == "input" and event.args.entity == "key_4194305_pressed":
				immediate_found = event.dur > 0.0
	check(immediate_found, "immediate menu key waited for physics")
	key(KEY_ESCAPE, false)
	key(KEY_ESCAPE, true)
	key(KEY_ESCAPE, false)
	player.set("auto_step", true)
	overlay.refresh()
	for kind in ["actor:", "script:", "quest:", "plugin:"]:
		check(overlay.label.text.contains(kind), "missing ranking " + kind)
	var fixture := OS.get_environment("SOE_PRF_13_FIXTURE")
	check(not fixture.is_empty(), "synthetic audio fixture directory required")
	var configuration := FileAccess.open(fixture.get_base_dir().path_join("Skyrim_Default.ini"), FileAccess.WRITE)
	check(configuration != null, "audio fixture INI open")
	if configuration != null:
		configuration.store_string("[General]\nsLanguage=ENGLISH\n")
		configuration.close()
	var wav := AudioStreamWAV.new()
	wav.format = AudioStreamWAV.FORMAT_16_BITS
	wav.mix_rate = 8000
	var samples := PackedByteArray()
	samples.resize(1600)
	for sample in 800:
		samples.encode_s16(sample * 2, int(sin(sample * 0.2) * 12000))
	wav.data = samples
	check(wav.save_to_wav(fixture.path_join("prf13.wav")) == OK, "audio fixture write")
	var bank: Node3D = ClassDB.instantiate("SoundBank")
	root.add_child(bank)
	var fixture_streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(fixture_streamer)
	player.call("set_entity_profiling", false)
	check(fixture_streamer.call("load_data", fixture), "audio fixture data open")
	check(bank.call("use_streamer_data", fixture_streamer), "audio fixture data binding")
	player.call("set_entity_profiling", true)
	var voice: int = bank.call("play_voice", "prf13.wav", Vector3.ZERO)
	check(voice >= 0, "synthetic voice start")
	if voice >= 0:
		bank.call("stop_voice", voice)
	bank.free()
	fixture_streamer.free()
	var built := 0
	var streamer: Node3D
	if not OS.get_environment("SKYRIM_DATA").is_empty():
		streamer = ClassDB.instantiate("CellStreamerNode")
		root.add_child(streamer)
		streamer.set("cell_occluders", false)
		player.call("set_entity_profiling", false)
		check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "installed data open")
		player.call("set_entity_profiling", true)
		streamer.call("begin_work_frame")
		built = streamer.call("stream_cell", "WhiterunBanneredMare")
		var counts: Dictionary = streamer.call("end_work_frame")
		check(built > 0 and counts.render_instances == built, "actual instance submission counts")
		player.call("set_entity_profiling", false)
		check(player.call("use_streamer_data", streamer), "installed player data binding")
		check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "installed player collision")
		check(player.call("start_gameplay"), "installed gameplay startup")
		player.call("set_entity_profiling", true)
		player.call("advance", 1.0 / 60.0)
		overlay.refresh()
		check(overlay.label.text.contains("actor:\n0:"), "installed actor ranking empty")
	var directory := ProjectSettings.globalize_path("user://")
	check(player.call("write_entity_profile", directory), "session export failed")
	var trace: Variant = JSON.parse_string(FileAccess.get_file_as_string(directory.path_join("soe-profile.json")))
	var summary: Variant = JSON.parse_string(FileAccess.get_file_as_string(directory.path_join("soe-entities.json")))
	check(trace is Dictionary and trace.has("traceEvents"), "invalid Chrome trace")
	check(summary is Dictionary and summary.has("traceEvents"), "invalid entity summary")
	var audio_found := false
	var render_found := built == 0
	var plugin_metrics_found := false
	if summary is Dictionary:
		for event: Dictionary in summary.traceEvents:
			if event.cat == "output" and event.name == "audio_voices.started":
				audio_found = event.args.calls == 1 and event.args.total_ns > 0
			if event.cat == "output" and event.name == "render_instances.submitted":
				render_found = event.args.calls >= built and event.args.render_instances == event.args.calls and event.args.total_ns > 0
			if event.cat == "plugin" and event.name == "<host>":
				plugin_metrics_found = event.args.has("total_ns") and event.args.has("self_ns") and event.args.has("wait_ns") and event.args.has("waits") and event.args.calls > 0
	check(audio_found, "voice start attribution absent")
	check(render_found, "render submission attribution absent")
	check(plugin_metrics_found, "plugin running and wait summary metrics absent")
	var found := false
	if trace is Dictionary:
		for event: Dictionary in trace.traceEvents:
			if event.cat == "input" and event.args.entity == "key_70_pressed":
				found = event.dur > 0.0
	check(found, "camera key presentation latency absent")
	await process_frame
	await RenderingServer.frame_post_draw
	var capture := OS.get_environment("SOE_PRF_13_CAPTURE")
	if not capture.is_empty():
		check(get_root().get_texture().get_image().save_png(capture) == OK, "overlay capture failed")
	key(KEY_F4, true)
	check(not player.call("entity_profiling_enabled"), "F4 did not end capture")
	if is_instance_valid(streamer):
		streamer.free()
	player.free()
	overlay.free()
	watchdog_timer.free()
	print("prf_13_f4_input_presentation_and_session_export: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
