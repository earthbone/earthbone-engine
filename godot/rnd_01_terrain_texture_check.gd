extends SceneTree

## RND-01 acceptance: a vanilla exterior cell submits textured terrain
## instances with a blended, non-white albedo image to the RenderingServer.
## Run with SKYRIM_DATA set and the soe-godot-host extension built.

var failures := 0


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		printerr("RESULT: FAIL: SKYRIM_DATA is not set")
		quit(2)
		return
	if not ClassDB.class_exists("CellStreamerNode"):
		var extension_path := ProjectSettings.globalize_path("res://soe.gdextension")
		var extension_status := GDExtensionManager.load_extension(extension_path)
		_check(extension_status == GDExtensionManager.LOAD_STATUS_OK,
			"soe-godot-host extension loads from %s (status %d)" % [extension_path, extension_status])
	if not ClassDB.class_exists("CellStreamerNode"):
		printerr("RESULT: FAIL: CellStreamerNode is not registered")
		quit(2)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "load_data opens installed data")
	var drawn: int = streamer.call("stream_exterior", "WhiterunOrigin", 1)
	_check(drawn > 0, "WhiterunOrigin submits exterior instances")
	var terrains: Array = streamer.call("terrain_surfaces")
	_check(not terrains.is_empty(), "terrain has RenderingServer instances")
	var textured := false
	var terrain_center := Vector3.ZERO
	for terrain in terrains:
		var rid: RID = terrain.get("rid")
		_check(rid.is_valid(), "terrain has a valid RenderingServer RID")
		var material = terrain.get("material")
		_check(material is StandardMaterial3D, "terrain uses a StandardMaterial3D")
		if not (material is StandardMaterial3D):
			continue
		var texture: Texture2D = material.albedo_texture
		if texture == null:
			continue
		var image := texture.get_image()
		if image == null or image.is_empty():
			continue
		textured = true
		terrain_center = terrain.get("center")
		break
	_check(textured, "a terrain surface has an uploaded albedo texture")
	if not textured or failures > 0:
		streamer.free()
		quit(1)
		return

	# Render the server-owned terrain into the main viewport. The camera frames
	# the chosen terrain instance, while neutral ambient light keeps the test
	# independent of a particular sun angle.
	var world_environment := WorldEnvironment.new()
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.02, 0.03, 0.04)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color(0.8, 0.8, 0.8)
	environment.ambient_light_energy = 1.0
	world_environment.environment = environment
	root.add_child(world_environment)

	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 42.0
	camera.position = terrain_center + Vector3(0.0, 100.0, 0.0)
	root.add_child(camera)
	camera.look_at(terrain_center, Vector3(0.0, 0.0, -1.0))
	camera.make_current()

	var light := DirectionalLight3D.new()
	light.rotation_degrees = Vector3(-45.0, 0.0, 0.0)
	light.light_energy = 0.8
	root.add_child(light)
	await process_frame
	await process_frame
	RenderingServer.force_draw(false, 0.1)
	var frame := root.get_texture().get_image()
	_check(frame != null and not frame.is_empty(), "main viewport produced a rendered frame")
	if frame != null and not frame.is_empty():
		print("terrain center=", terrain_center, " camera=", camera.global_position,
			" frame=", frame.get_width(), "x", frame.get_height())
		var colors := {}
		var visible_terrain_pixels := 0
		var background := environment.background_color
		for y in range(0, frame.get_height(), 3):
			for x in range(0, frame.get_width(), 3):
				var color := frame.get_pixel(x, y)
				var rgb_delta := absf(color.r - background.r) + absf(color.g - background.g)
				rgb_delta += absf(color.b - background.b)
				if rgb_delta > 0.3:
					visible_terrain_pixels += 1
					colors[color.to_html(false)] = true
		_check(visible_terrain_pixels > 100, "viewport contains visible terrain pixels")
		_check(colors.size() > 4, "rendered terrain pixels contain blended ground colors")
		_check(colors.keys().any(func(color: String) -> bool: return color != "ffffff"),
			"rendered terrain pixels are not all white")
		print("viewport pixels: visible=", visible_terrain_pixels, " distinct_colors=", colors.size())
	light.free()
	camera.free()
	world_environment.free()
	streamer.free()
	if failures == 0:
		print("RESULT: PASS: vanilla exterior terrain draws its blended texture")
		quit(0)
	else:
		printerr("RESULT: FAIL: %d check(s) failed" % failures)
		quit(1)
