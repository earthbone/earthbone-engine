extends SceneTree

## RND-20 production material checks, with pixel thresholds fixed before the run.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var fixture := OS.get_environment("SOE_RND_20_FIXTURE")
	var data := OS.get_environment("SKYRIM_DATA")
	if fixture.is_empty() or data.is_empty() or DisplayServer.get_name() == "headless":
		printerr("RND-20 requires fixture, installed data and a renderer")
		quit(2)
		return
	var viewport := SubViewport.new()
	viewport.size = Vector2i(256, 256)
	viewport.own_world_3d = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	get_root().add_child(viewport)
	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 1.8
	camera.position = Vector3(0, 2, 0)
	viewport.add_child(camera)
	camera.look_at(Vector3.ZERO, Vector3.FORWARD)
	var lighting := WorldEnvironment.new()
	lighting.environment = Environment.new()
	lighting.environment.background_mode = Environment.BG_COLOR
	lighting.environment.background_color = Color(0.1, 0.2, 0.3)
	lighting.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	lighting.environment.ambient_light_color = Color.WHITE
	lighting.environment.ambient_light_energy = 1.0
	lighting.environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
	viewport.add_child(lighting)
	var host := CellStreamerNode.new()
	viewport.add_child(host)
	_check(host.load_data(fixture), "synthetic data load")
	for i in range(2):
		host.clear()
		_check(host.stream_cell("Fallback%d" % i) == 1, "only visible control is drawn")
		var stats: Dictionary = host.material_albedo_stats()
		_check(int(stats.get("missing", -1)) == 0, "albedo upload failed")
		_check(int(stats.get("checker", 0)) >= 1, "production material did not bind checker")
		for frame in range(4):
			await process_frame
			RenderingServer.force_draw(false)
		var pixels := viewport.get_texture().get_image()
		var magenta := 0
		var black := 0
		for y in range(32, 224):
			for x in range(32, 224):
				var color := pixels.get_pixel(x, y)
				if color.r > 0.4 and color.b > 0.4 and color.g < 0.15:
					magenta += 1
				if color.r < 0.05 and color.g < 0.05 and color.b < 0.05:
					black += 1
		_check(magenta > 500 and black > 500, "checker must visibly contain both colors")
		pixels.save_png(fixture.path_join("checker%d.png" % i))
		print("rnd_20_checker_pixels path=", i, " magenta=", magenta, " black=", black, " bindings=", stats)
	host.free()
	viewport.free()
	var world_host := CellStreamerNode.new()
	get_root().add_child(world_host)
	_check(world_host.load_data(data), "installed data load")
	for origin in ["WhiterunOrigin", "WhiterunExterior04"]:
		world_host.clear()
		_check(world_host.stream_exterior(origin, 1) > 0, "gate block must render")
		var stats: Dictionary = world_host.material_albedo_stats()
		_check(int(stats.get("missing", -1)) == 0, "gate material lacks named albedo")
		_check(int(stats.get("bound", 0)) > 10, "gate block must bind ordinary textures")
		_check(int(stats.get("checker", -1)) == 0, "vanilla gate unexpectedly needs fallback")
		print("rnd_20_gate_bindings origin=", origin, " stats=", stats)
	world_host.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " RND-20 albedo, checker pixels and gate scenes")
	quit(0 if failures == 0 else 1)
