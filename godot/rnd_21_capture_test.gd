extends SceneTree

## Installed-data capture check for RND-21. Brightness bounds are fixed before
## the first capture: average linear luminance 0.02 to 0.65, fewer than one
## percent fully clipped pixels.
const Warmup := preload("res://render_warmup.gd")
const MIN_AVERAGE := 0.02
const MAX_AVERAGE := 0.65
const MAX_CLIPPED_RATIO := 0.01
const CAPTURE := "res://../target/rnd_21_bannered_mare.png"

var player: Node3D
var frames := 0
var loaded := false


func _initialize() -> void:
	call_deferred("_start")


func _start() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("RND-21 requires SKYRIM_DATA and the built host extension")
		quit(1)
		return
	var scene := Node3D.new()
	get_root().add_child(scene)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	scene.add_child(streamer)
	if not streamer.call("begin_load_data", data):
		printerr("RND-21 could not start loading installed data")
		quit(1)
		return
	var state: int = streamer.call("poll_load_data")
	while state == 0:
		await process_frame
		state = streamer.call("poll_load_data")
	if state < 0 or streamer.call("stream_cell", "WhiterunBanneredMare") <= 0:
		printerr("RND-21 could not stream the Bannered Mare")
		quit(1)
		return
	var lighting: Dictionary = streamer.call("cell_lighting", "WhiterunBanneredMare")
	if lighting.is_empty():
		printerr("RND-21 Bannered Mare has no resolved cell lighting")
		quit(1)
		return
	_apply_lighting(scene, lighting)
	player = ClassDB.instantiate("PlayerNode")
	scene.add_child(player)
	if not player.call("use_streamer_data", streamer) or player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) <= 0:
		printerr("RND-21 could not enter the Bannered Mare")
		quit(1)
		return
	var rendering: Dictionary = await Warmup.prepare()
	if not rendering.error.is_empty():
		printerr("RND-21 rendering preparation failed: " + rendering.error)
		quit(1)
		return
	await process_frame
	await process_frame
	loaded = true


func _apply_lighting(scene: Node3D, values: Dictionary) -> void:
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.05, 0.05, 0.07)
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


func _process(_delta: float) -> bool:
	if not loaded:
		return false
	frames += 1
	if frames < 120:
		return false
	var image := get_root().get_texture().get_image()
	var capture := ProjectSettings.globalize_path(CAPTURE)
	if image.save_png(capture) != OK:
		printerr("RND-21 could not save the capture")
		quit(1)
		return false
	var luminance_sum := 0.0
	var clipped := 0
	var total := image.get_width() * image.get_height()
	for y in image.get_height():
		for x in image.get_width():
			var pixel := image.get_pixel(x, y)
			luminance_sum += 0.2126 * pow(pixel.r, 2.2) + 0.7152 * pow(pixel.g, 2.2) + 0.0722 * pow(pixel.b, 2.2)
			if pixel.r >= 0.98 and pixel.g >= 0.98 and pixel.b >= 0.98:
				clipped += 1
	var average := luminance_sum / float(total)
	var clipped_ratio := float(clipped) / float(total)
	print("RND-21 average_linear_luminance=%.4f clipped_pixel_ratio=%.5f" % [average, clipped_ratio])
	DirAccess.remove_absolute(capture)
	var ok := average >= MIN_AVERAGE and average <= MAX_AVERAGE and clipped_ratio < MAX_CLIPPED_RATIO
	quit(0 if ok else 1)
	return false
