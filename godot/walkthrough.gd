extends SceneTree

## Gameplay integration prototype. Requires SKYRIM_DATA and the release extension.
## WASD move, mouse looks, Space jumps, Ctrl sneaks, F changes camera, Esc exits.
const Warmup := preload("res://render_warmup.gd")
const SystemOverlay := preload("res://system_overlay.gd")
const RecordSky := preload("res://record_sky.gd")

var player: Node3D
var system_overlay: CanvasLayer
var frames := 0
var loaded := false
var loading_label: Label
var startup_msec := 0
var loading_phase := "Opening game data"

func _initialize() -> void:
	call_deferred("_start")

func _finalize() -> void:
	print("WALKTHROUGH stopped: ready=", loaded)

func _start() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("Set SKYRIM_DATA and build/import the Godot extension first")
		quit(1)
		return
	var scene := Node3D.new()
	get_root().add_child(scene)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	scene.add_child(streamer)
	loading_label = Label.new()
	loading_label.z_index = 1
	loading_label.position = Vector2(24, 24)
	get_root().add_child(loading_label)
	startup_msec = Time.get_ticks_msec()
	if not streamer.call("begin_load_data", data):
		printerr("Could not start data loading")
		quit(1)
		return
	var state: int = streamer.call("poll_load_data")
	while state == 0:
		await process_frame
		state = streamer.call("poll_load_data")
	print("STARTUP data ms=", Time.get_ticks_msec() - startup_msec)
	loading_phase = "Building the Bannered Mare"
	await process_frame
	await process_frame
	var phase_msec := Time.get_ticks_msec()
	if state < 0 or streamer.call("stream_cell", "WhiterunBanneredMare") <= 0:
		printerr("Could not render the Bannered Mare")
		quit(1)
		return
	print("STARTUP render ms=", Time.get_ticks_msec() - phase_msec)
	_apply_cell_lighting(scene, streamer.call("cell_lighting", "WhiterunBanneredMare"))
	player = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", OS.get_environment("SOE_CAPTURE_PATH").is_empty())
	scene.add_child(player)
	system_overlay = SystemOverlay.new()
	system_overlay.player = player
	get_root().add_child(system_overlay)
	phase_msec = Time.get_ticks_msec()
	var spawn := Vector3(0.0, 0.0, 500.0)
	if OS.get_environment("SOE_AUD_11_TEST") == "1":
		spawn = Vector3(-125.0, -800.0, 120.0)
	if not player.call("use_streamer_data", streamer) or player.call("enter_cell", "WhiterunBanneredMare", spawn.x, spawn.y, spawn.z) <= 0:
		printerr("Could not enter the Bannered Mare")
		quit(1)
		return
	print("STARTUP player ms=", Time.get_ticks_msec() - phase_msec)
	loading_phase = "Preparing rendering"
	var rendering := {"error": "", "load_render_frames": 0, "load_render_ms": 0.0}
	if DisplayServer.get_name() != "headless":
		var loading_cover := Warmup.cover(get_root())
		rendering = await Warmup.prepare()
		if not rendering.error.is_empty():
			printerr("Could not prepare rendering: ", rendering.error)
			quit(1)
			return
		loading_cover.queue_free()
	print("STARTUP rendering ", JSON.stringify(rendering))
	loading_phase = "Starting game scripts"
	await process_frame
	await process_frame
	if not player.call("start_gameplay"):
		printerr("Could not start the gameplay runtime")
		quit(1)
		return
	print("STARTUP ready ms=", Time.get_ticks_msec() - startup_msec)
	var sky := RecordSky.new()
	scene.add_child(sky)
	if not sky.use_player(player):
		printerr("Could not initialize the record sky")
		quit(1)
		return
	loading_label.queue_free()
	loading_label = null
	print("WASD move, mouse look, E activate, Space jump, Ctrl sneak, F camera, F3 system timings, Esc exit")
	loaded = true
	if OS.get_environment("SOE_AUD_11_TEST") == "1":
		await _run_aud_11_check()

func _run_aud_11_check() -> void:
	for _frame in range(120):
		await process_frame
	if not player.call("is_on_ground"):
		printerr("AUD-11 player did not settle on the Bannered Mare floor")
		quit(1)
		return
	var capture := AudioEffectCapture.new()
	capture.buffer_length = 0.25
	AudioServer.add_bus_effect(0, capture)
	var walk_start: Vector3 = player.call("position")
	var forward := InputEventKey.new()
	forward.keycode = KEY_W
	forward.pressed = true
	Input.parse_input_event(forward)
	Input.flush_buffered_events()
	var peak := 0.0
	var started := Time.get_ticks_msec()
	while Time.get_ticks_msec() - started < 20_000:
		await process_frame
		var frames := capture.get_frames_available()
		if frames > 0:
			for sample: Vector2 in capture.get_buffer(frames):
				peak = maxf(peak, maxf(absf(sample.x), absf(sample.y)))
	forward.pressed = false
	Input.parse_input_event(forward)
	Input.flush_buffered_events()
	var walk_seconds := (Time.get_ticks_msec() - started) / 1000.0
	var bank := player.get_node_or_null("SoundBank")
	var walk_distance := walk_start.distance_to(player.call("position"))
	var voice_requested := _aud_11_start_voice()
	var voice_started := Time.get_ticks_msec()
	while voice_requested and Time.get_ticks_msec() - voice_started < 2_000:
		await process_frame
		var frames := capture.get_frames_available()
		if frames > 0:
			for sample: Vector2 in capture.get_buffer(frames):
				peak = maxf(peak, maxf(absf(sample.x), absf(sample.y)))
	var footstep_starts := int(player.call("footstep_count"))
	var ambience_starts := -1
	var music_starts := -1
	var voice_starts := -1
	if bank != null:
		ambience_starts = int(bank.call("ambience_start_count"))
		music_starts = int(bank.call("music_start_count"))
		voice_starts = int(bank.call("voice_start_count"))
		if voice_requested and voice_starts == 0:
			printerr("AUD-11 voice did not start: ", bank.call("last_error"))
	var peak_db := -INF if peak <= 0.0 else 20.0 * log(peak) / log(10.0)
	print("AUD-11 systems footsteps=", footstep_starts, " ambience=", ambience_starts,
		" music=", music_starts, " voices=", voice_starts, " master_peak_db=", peak_db,
		" walk_seconds=", walk_seconds,
		" walk_distance=", walk_distance)
	var passed := footstep_starts > 0 and ambience_starts > 0 and music_starts > 0 \
		and voice_requested and voice_starts > 0 and peak_db > -60.0 and walk_distance > 10.0
	print("AUD-11 RESULT: ", "PASS" if passed else "FAIL")
	quit(0 if passed else 1)

func _aud_11_start_voice() -> bool:
	# CONTEXT: A straight unattended walk does not request a dialogue response.
	var move := {"action": "move_to", "file": 0, "object": 0x1A66E,
		"offset": Vector3(0.0, -150.0, 0.0), "match_rotation": false}
	if not player.call("queue_player_input", move) or not player.call("step_player_input"):
		printerr("AUD-11 could not approach Hulda")
		return false
	if not player.call("open_dialogue_menu", 0, 0x1A66E):
		printerr("AUD-11 could not open Hulda's dialogue")
		return false
	var content := player.get_node("MenuLayer/MenuPanel/Layout/Content")
	if content.get_child_count() > 0:
		var rows := content.get_child(0).get_child(0)
		for option in rows.get_children():
			if option is Button:
				print("AUD-11 dialogue topic: ", option.text)
				option.emit_signal("pressed")
				return true
	printerr("AUD-11 Hulda did not offer a dialogue topic")
	return false

func _process(_delta: float) -> bool:
	if is_instance_valid(loading_label):
		loading_label.text = "%s... %.1f s\nEsc exits" % [loading_phase, (Time.get_ticks_msec() - startup_msec) / 1000.0]
	if loaded:
		frames += 1
		var capture := OS.get_environment("SOE_CAPTURE_PATH")
		if frames == 120 and not capture.is_empty():
			var error := get_root().get_texture().get_image().save_png(capture)
			print("CAPTURE: ", capture, " error=", error)
			quit(0 if error == OK else 1)
	if Input.is_key_pressed(KEY_ESCAPE):
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
		quit(0)
	return false

func _apply_cell_lighting(scene: Node3D, values: Dictionary) -> void:
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.05, 0.05, 0.07)
	if values.is_empty():
		printerr("Cell has no authored XCLL or inherited LGTM lighting")
		quit(1)
		return
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = values["ambient"]
	environment.ambient_light_energy = 1.0
	environment.fog_enabled = values["fog_far"] > values["fog_near"]
	environment.fog_light_color = values["fog_near_color"]
	environment.fog_depth_begin = maxf(values["fog_near"], 0.0)
	var fog_end: float = values["fog_far"]
	if values["fog_clip_distance"] > 0.0:
		fog_end = minf(fog_end, values["fog_clip_distance"])
	environment.fog_depth_end = maxf(fog_end, environment.fog_depth_begin + 0.01)
	environment.fog_density = clampf(values["fog_power"] * values["fog_max"], 0.0, 1.0)
	var world := WorldEnvironment.new()
	world.environment = environment
	scene.add_child(world)
	var light := DirectionalLight3D.new()
	light.name = "CellDirectionalAmbient"
	light.light_color = values["directional"]
	light.light_energy = clampf(values["directional_fade"], 0.0, 1.0)
	light.rotation = Vector3(deg_to_rad(values["rotation_xy"]), 0.0, deg_to_rad(values["rotation_z"]))
	light.distance_fade_enabled = values["light_fade_end"] > values["light_fade_begin"]
	light.distance_fade_begin = values["light_fade_begin"]
	light.distance_fade_length = maxf(values["light_fade_end"] - values["light_fade_begin"], 0.0)
	light.shadow_enabled = false
	scene.add_child(light)
	var rotations := [
		Vector3(0.0, -PI / 2.0, 0.0), Vector3(0.0, PI / 2.0, 0.0),
		Vector3(PI / 2.0, 0.0, 0.0), Vector3(-PI / 2.0, 0.0, 0.0),
		Vector3(0.0, PI, 0.0), Vector3.ZERO,
	]
	var ambient_directions: Array = values["ambient_directions"]
	for index in ambient_directions.size():
		var ambient := DirectionalLight3D.new()
		ambient.name = "CellAmbientDirection%d" % index
		ambient.light_color = ambient_directions[index]
		ambient.light_energy = 1.0 / 6.0
		ambient.shadow_enabled = false
		ambient.rotation = rotations[index]
		scene.add_child(ambient)
