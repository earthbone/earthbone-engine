extends "res://walkthrough.gd"

## RND-35 walkthrough coverage. Zero magenta pixels may cover uploaded LAND.
## Each diagnostic must also show at least 1000 LOD pixels outside that grid.
const CELL_METRES := 58.5216
const MIN_LOD_PIXELS := 1000
var failures := 0
var camera: Camera3D
var checking := false

func _initialize() -> void:
	startup_cell = "WhiterunOrigin"
	startup_radius = 2
	super._initialize()

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _process(_delta: float) -> bool:
	if loaded and not checking:
		checking = true
		call_deferred("rnd_35_walkthrough_lod_capture")
	return false

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("RND-35 FAIL: ", message)

func _capture(label: String) -> Image:
	for _frame in range(4):
		await process_frame
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	_check(image.save_png("res://.godot/rnd-35-" + label + ".png") == OK, "save " + label)
	return image

func _coverage(label: String) -> void:
	# CONTEXT: Mesh coordinates are an independent oracle for the streamer's mask.
	var covered := {}
	var low := Vector2i(2147483647, 2147483647)
	var high := Vector2i(-2147483647, -2147483647)
	for mesh: ArrayMesh in streamer.call("loaded_terrain_meshes"):
		var bounds := mesh.get_aabb()
		var coord := Vector2i(roundi(bounds.position.x / CELL_METRES), roundi(-bounds.end.z / CELL_METRES))
		covered[coord] = true
		low = Vector2i(mini(low.x, coord.x), mini(low.y, coord.y))
		high = Vector2i(maxi(high.x, coord.x), maxi(high.y, coord.y))
	_check(covered.size() >= 25, label + " has fewer than 25 uploaded LAND cells")
	if covered.is_empty():
		return
	print("RND-35 covered cells: ", covered.keys())
	print("RND-35 streamer mask: ", streamer.call("terrain_lod_stats").get("covered_cells", ""))
	# CONTEXT: Integer pixels per cell keep sample centres away from shared edges.
	var center := Vector3(roundf((low.x + high.x + 1.0) * 0.5) * CELL_METRES, 0, -roundf((low.y + high.y + 1.0) * 0.5) * CELL_METRES)
	var span := maxf(high.x - low.x + 5.0, high.y - low.y + 5.0)
	var pixels_per_cell := maxi(1, floori(root.size.y / span))
	camera.size = CELL_METRES * root.size.y / pixels_per_cell
	camera.position = center + Vector3(0, 1500, 0)
	camera.look_at(center, Vector3(0, 0, -1))
	camera.make_current()
	streamer.call("set_terrain_lod_debug_color", false)
	await _capture(label + "-normal")
	streamer.call("set_terrain_lod_debug_color", true)
	# CONTEXT: The hidden-LAND control catches overlapping LOD below the ground too.
	for hidden in [false, true]:
		streamer.call("set_loaded_terrain_visible", not hidden)
		streamer.call("set_terrain_lod_visible", false)
		var control: Image = await _capture(label + ("-hidden-land-control" if hidden else "-control"))
		streamer.call("set_terrain_lod_visible", true)
		var image: Image = await _capture(label + ("-lod-only" if hidden else "-coverage"))
		var overlap := 0
		var overlaps := {}
		var outside := 0
		for y in image.get_height():
			for x in image.get_width():
				var pixel := image.get_pixel(x, y)
				var previous := control.get_pixel(x, y)
				var changed := absf(pixel.r - previous.r) + absf(pixel.g - previous.g) + absf(pixel.b - previous.b) > 0.2
				if changed and pixel.r > 0.8 and pixel.g < 0.1 and pixel.b > 0.8:
					var ray := camera.project_ray_origin(Vector2(x + 0.5, y + 0.5))
					var cell := Vector2i(floori(ray.x / CELL_METRES), floori(-ray.z / CELL_METRES))
					if covered.has(cell):
						overlap += 1
						overlaps[cell] = int(overlaps.get(cell, 0)) + 1
					else:
						outside += 1
		_check(overlap == 0, label + " LOD pixels inside uploaded grid: " + str(overlap))
		_check(outside >= MIN_LOD_PIXELS, label + " lacks visible LOD control")
		print("RND-35 overlaps: ", overlaps, " bounds=", low, " to=", high, " size=", camera.size)
		print("RND-35 capture: stop=", label, " hidden_land=", hidden, " cells=", covered.size(), " overlap=", overlap, " outside_lod=", outside)
	streamer.call("set_loaded_terrain_visible", true)
	streamer.call("set_terrain_lod_debug_color", false)

func _move(reference: Dictionary, offset: Vector3) -> void:
	_check(player.call("queue_player_input", {"action": "move_to", "file": reference.file, "object": reference.object, "offset": offset, "match_rotation": false}), "queue movement")
	_check(player.call("step_player_input"), "apply movement")
	for _frame in range(12):
		player.call("advance", 0.0)

func _cross_border(border: int) -> void:
	var before: Vector3 = player.call("position")
	var before_draws: Dictionary = streamer.call("cell_instances")
	var cell_x := floori(before.x / 4096.0)
	# CONTEXT: Moving relative to the player retains the current CELL until the grid update.
	_move({"file": 0, "object": 0x14}, Vector3(-4096, 0, 0))
	var after: Vector3 = player.call("position")
	_check(floori(after.x / 4096.0) == cell_x - 1, "cross border " + str(border))
	var after_draws: Dictionary = streamer.call("cell_instances")
	var retained := 0
	for cell in before_draws:
		if after_draws.has(cell):
			retained += 1
			_check(before_draws[cell] == after_draws[cell], "retained draw resources changed for " + cell)
	_check(retained > 0 and before_draws.keys() != after_draws.keys(), "movement applies a cell delta")
	print("RND-35 movement: border=", border, " from=", before, " to=", after, " retained=", retained)

func rnd_35_walkthrough_lod_capture() -> void:
	# CONTEXT: Remove the metrics exit callback while its viewport is still live.
	system_overlay.free()
	for overlay in root.find_children("*", "CanvasLayer", true, false):
		overlay.hide()
	for sky in root.find_children("*", "Node", true, false):
		if sky.get_script() == RecordSky:
			sky.set_process(false)
			sky.hide()
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0, 1, 1)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color.WHITE
	environment.ambient_light_energy = 0.7
	root.world_3d.environment = environment
	for world_environment: WorldEnvironment in root.find_children("*", "WorldEnvironment", true, false):
		world_environment.environment = environment
	camera = Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.far = 3000.0
	root.add_child(camera)
	player.call("advance", 0.0)
	await _coverage("startup")
	if failures > 0:
		quit(1)
		return
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "Whiterun city gate")
	if not gate.is_empty():
		var before: Vector3 = player.call("position")
		_move(gate, Vector3(0, 0, 1200))
		var at_gate: Vector3 = player.call("position")
		print("RND-35 city movement: from=", before, " to=", at_gate)
		await _coverage("city-gate")
		_check(player.call("activate_reference", gate.file, gate.object), "cross gate to Tamriel")
		player.call("advance", 0.0)
		_check(player.call("detection_worldspace") == "Tamriel", "Tamriel destination")
		var far_gate := {"file": gate.far_file, "object": gate.far_object}
		_move(far_gate, Vector3(-4096, -4096, 1200))
		await _coverage("plains")
		for border in [1, 2]:
			_cross_border(border)
			await _coverage("border-" + str(border))
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", ": RND-35 walkthrough LOD coverage")
	quit(0 if failures == 0 else 1)
