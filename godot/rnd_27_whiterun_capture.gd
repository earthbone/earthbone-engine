extends SceneTree

## RND-27 installed gameplay residency and frozen city/plains captures.
const RecordSky := preload("res://record_sky.gd")
const TERRAIN_PIXELS := 500
const SILHOUETTE_PIXELS := 1000
const SILHOUETTE_COLUMNS := 120
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(value: bool, message: String) -> void:
	if not value:
		failures += 1
		printerr("RND-27 FAIL: ", message)

func _position(value: Vector3) -> Vector3:
	return Vector3(value.x, value.z, -value.y) / 70.0

func _capture(path: String) -> Image:
	# CONTEXT: Frozen simulation also freezes cold-load fades over the geometry.
	for overlay in root.find_children("*", "CanvasLayer", true, false):
		overlay.hide()
	for _frame in range(8):
		await process_frame
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	_check(image.save_png(path) == OK, "save " + path)
	return image

func _difference(image: Image, control: Image, area: Rect2i) -> Vector2i:
	var pixels := 0
	var columns := 0
	for x in range(area.position.x, area.end.x):
		var changed := 0
		for y in range(area.position.y, area.end.y):
			var a := image.get_pixel(x, y)
			var b := control.get_pixel(x, y)
			if maxf(absf(a.r - b.r), maxf(absf(a.g - b.g), absf(a.b - b.b))) > 0.04:
				changed += 1
		pixels += changed
		if changed >= 3:
			columns += 1
	return Vector2i(pixels, columns)

func _move(player: Node3D, reference: Dictionary, offset: Vector3) -> void:
	_check(player.call("queue_player_input", {"action": "move_to", "file": reference.file, "object": reference.object, "offset": offset, "match_rotation": false}), "queue movement")
	_check(player.call("step_player_input"), "apply movement")
	player.call("advance", 0.0)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("RND-27 FAIL: installed data and host extension required")
		quit(2)
		return
	var directory := "res://.godot/rnd_27"
	DirAccess.make_dir_recursive_absolute(directory)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "load installed data")
	var city_cells: Array[Dictionary] = []
	for cell: Dictionary in streamer.call("editor_cell_tree"):
		if cell.get("world_name", "") == "WhiterunWorld" and not cell.get("interior", true):
			city_cells.append(cell)
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "share data")
	_check(player.call("enter_cell", "WhiterunOrigin", 28313.0, 1734.0, -1000.0) >= 0, "enter Dragonsreach exterior")
	_check(player.call("start_gameplay"), "start gameplay")
	player.call("advance", 0.0)
	var resident: Dictionary = player.call("exterior_resources")
	_check(city_cells.size() > 9, "installed city spans more than the former near square")
	for cell in city_cells:
		_check(resident.has(cell.id), "missing city cell " + String(cell.id))
	for kind in ["terrain", "object"]:
		var stats: Dictionary = streamer.call(kind + "_lod_stats")
		_check(stats.get("world", "") == "Tamriel", kind + " parent asset world")
		_check(int(stats.get("instances", 0)) > 0, kind + " parent instances")
		_check(int(stats.get("failed", -1)) == 0, kind + " parent blocks failed")
	print("RND-27 city cells=", city_cells.size(), " resident=", resident.size())
	var sky := RecordSky.new()
	root.add_child(sky)
	sky.set_process(false)
	_check(sky.use_streamer(streamer, "WhiterunOrigin", 12.0), "record sky")
	_check(sky.records.set_weather("SkyrimClear", true) and sky.refresh(), "clear noon")
	var camera := Camera3D.new()
	root.add_child(camera)
	camera.far = 4000.0
	camera.position = _position(Vector3(28313, 800, -1000))
	camera.look_at(_position(Vector3(25000, -40000, -4500)))
	camera.make_current()
	var plains := await _capture(directory + "/dragonsreach-plains.png")
	streamer.call("set_terrain_lod_visible", false)
	var plains_control := await _capture(directory + "/dragonsreach-no-terrain-lod.png")
	streamer.call("set_terrain_lod_visible", true)
	var terrain := _difference(plains, plains_control, Rect2i(0, 90, 640, 200))
	_check(terrain.x >= TERRAIN_PIXELS, "Dragonsreach plains lack parent terrain")
	print("RND-27 plains terrain_pixels=", terrain.x)
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "city gate")
	if not gate.is_empty():
		_move(player, gate, Vector3(0, 0, 300))
		var moved: Dictionary = player.call("exterior_resources")
		_check(moved.keys() == resident.keys(), "city resources unload while walking to gate")
		_check(player.call("activate_reference", gate.file, gate.object), "cross gate")
		player.call("advance", 0.0)
		_check(player.call("detection_worldspace") == "Tamriel", "outside world")
		var near: Dictionary = player.call("exterior_resources")
		_check(near.size() >= 25, "outside near coverage is smaller than 5 by 5")
		for cell in city_cells:
			_check(not near.has(cell.id), "city resources survive world change")
		camera.position = _position(Vector3(18000, -17000, -3400))
		camera.look_at(_position(Vector3(24500, -3500, -3000)))
		camera.make_current()
		var city := await _capture(directory + "/outside-gate-city.png")
		# CONTEXT: The control removes the district cells and distant objects together.
		for y in range(-3, 2):
			for x in range(4, 8):
				streamer.call("release_exterior_cell", x, y)
		streamer.call("clear_object_lod")
		var city_control := await _capture(directory + "/outside-gate-no-city.png")
		var silhouette := _difference(city, city_control, Rect2i(80, 40, 480, 140))
		_check(silhouette.x >= SILHOUETTE_PIXELS and silhouette.y >= SILHOUETTE_COLUMNS, "outside gate lacks full city silhouette")
		print("RND-27 city silhouette_pixels=", silhouette.x, " columns=", silhouette.y)
	player.free()
	streamer.free()
	sky.free()
	camera.free()
	print("RND-27 RESULT: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
