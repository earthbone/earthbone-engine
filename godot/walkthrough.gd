extends SceneTree

## Gameplay integration prototype. Requires SKYRIM_DATA and the release extension.
## WASD move, mouse looks, Space jumps, Ctrl sneaks, F changes camera, Esc exits.
const Warmup := preload("res://render_warmup.gd")
const SystemOverlay := preload("res://system_overlay.gd")

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
	if not player.call("use_streamer_data", streamer) or player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) <= 0:
		printerr("Could not enter the Bannered Mare")
		quit(1)
		return
	print("STARTUP player ms=", Time.get_ticks_msec() - phase_msec)
	loading_phase = "Preparing rendering"
	var loading_cover := Warmup.cover(get_root())
	var rendering: Dictionary = await Warmup.prepare()
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
	loading_label.queue_free()
	loading_label = null
	print("WASD move, mouse look, E activate, Space jump, Ctrl sneak, F camera, F3 system timings, Esc exit")
	loaded = true

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
