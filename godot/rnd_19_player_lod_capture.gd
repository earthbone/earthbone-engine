extends SceneTree

## RND-19 gameplay LOD handoff, world changes and an installed mountain capture.
const CELL := "WhiterunBanneredMare"
const TERRAIN_PIXEL_THRESHOLD := 500
var failures := 0
var cells := {}

func _initialize() -> void:
	call_deferred("rnd_19_player_lod_capture")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _lod_contract(player: Node3D, streamer: Node3D, radius: int) -> void:
	var position: Vector3 = player.call("position")
	var focus := Vector2i(floori(position.x / 4096.0), floori(position.y / 4096.0))
	var near: Dictionary = player.call("exterior_resources")
	_check(not near.is_empty(), "no full-resolution exterior cells")
	for kind in ["terrain", "object"]:
		var stats: Dictionary = streamer.call(kind + "_lod_stats")
		_check(stats.get("world", "") == "Tamriel", kind + " LOD asset world")
		_check(int(stats.get("lod_radius", 0)) == radius, kind + " configured LOD radius")
		_check(int(stats.get("full_res_radius", 0)) >= 2, kind + " ignores retained full cells")
		_check(int(stats.get("instances", 0)) > 0, kind + " has no rendered instances")
		_check(int(stats.get("failed", -1)) == 0, kind + " failed block load")
		for entry in String(stats.get("blocks", "")).split(" ", false):
			var values := entry.split(":")
			var span := int(values[0])
			var origin := Vector2i(int(values[1]), int(values[2]))
			var nearest := Vector2i(clampi(focus.x, origin.x, origin.x + span - 1), clampi(focus.y, origin.y, origin.y + span - 1))
			_check(maxi(absi(nearest.x - focus.x), absi(nearest.y - focus.y)) <= radius, kind + " exceeds LOD radius")
			for id in near:
				if cells.has(id):
					var coord: Vector2i = cells[id]
					if kind == "object":
						_check(not (coord.x >= origin.x and coord.x < origin.x + span and coord.y >= origin.y and coord.y < origin.y + span), kind + " double draws full cell " + id)
	_check(not String(streamer.call("terrain_lod_stats").get("covered_cells", "")).is_empty(), "terrain handoff has no loaded cell mask")

func _move(player: Node3D, reference: Dictionary, offset: Vector3) -> void:
	_check(player.call("queue_player_input", {"action": "move_to", "file": reference.file, "object": reference.object, "offset": offset, "match_rotation": false}), "queue player movement")
	_check(player.call("step_player_input"), "apply player movement")
	player.call("advance", 0.0)

func rnd_19_player_lod_capture() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var capture := OS.get_environment("SOE_RND19_CAPTURE_PATH")
	if data.is_empty() or capture.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: SKYRIM_DATA, capture path and host extension are required")
		quit(2)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "installed data load")
	for cell: Dictionary in streamer.call("editor_cell_tree"):
		if not cell.get("interior", true) and not String(cell.get("world_id", "")).is_empty():
			cells[cell.id] = Vector2i(int(cell.x), int(cell.y))
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	player.set("exterior_lod_radius", 16)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0, "Bannered Mare entry")
	_check(player.call("start_gameplay"), "gameplay runtime")
	if failures > 0:
		player.free()
		streamer.free()
		quit(1)
		return
	var interior := String(player.call("gameplay_cell"))
	var door: Dictionary = {}
	for candidate: Dictionary in player.call("gameplay_doors"):
		if not candidate.locked and candidate.get("far_worldspace", "") == "WhiterunWorld":
			door = candidate
			break
	_check(not door.is_empty(), "Bannered Mare exterior door")
	if door.is_empty():
		player.free()
		streamer.free()
		quit(1)
		return
	_check(player.call("activate_reference", door.file, door.object), "enter WhiterunWorld")
	player.call("advance", 0.0)
	_check(player.call("detection_worldspace") == "WhiterunWorld", "city worldspace")
	_lod_contract(player, streamer, 16)
	for _frame in range(45):
		player.call("advance", 1.0 / 60.0)
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "Whiterun gate")
	if not gate.is_empty():
		_move(player, gate, Vector3(0, 0, 1200))
		_check(gate.has("far_file"), "gate destination reference")
		player.call("set_facing", PI, 0.0)
		for _frame in range(10):
			await process_frame
		# CONTEXT: Frozen gameplay isolates the terrain contribution from near objects.
		streamer.call("set_terrain_lod_visible", false)
		await process_frame
		await process_frame
		var control := root.get_texture().get_image()
		streamer.call("set_terrain_lod_visible", true)
		await process_frame
		await process_frame
		var image := root.get_texture().get_image()
		_check(image.save_png(capture) == OK, "save mountain capture")
		_check(control.save_png(capture.trim_suffix(".png") + "-no-terrain.png") == OK, "save terrain control")
		var terrain_pixels := 0
		for y in range(int(image.get_height() / 3), int(image.get_height() * 2 / 3)):
			for x in range(image.get_width()):
				var a := image.get_pixel(x, y)
				var b := control.get_pixel(x, y)
				if absf(a.r - b.r) + absf(a.g - b.g) + absf(a.b - b.b) > 0.12:
					terrain_pixels += 1
		_check(terrain_pixels >= TERRAIN_PIXEL_THRESHOLD, "distant region has no measurable terrain contribution")
		print("RND-19 mountains: terrain_pixels=", terrain_pixels, " threshold=", TERRAIN_PIXEL_THRESHOLD, " capture=", capture)
		player.set("exterior_lod_radius", 12)
		_check(player.call("activate_reference", gate.file, gate.object), "cross to Tamriel")
		player.call("advance", 1.0 / 60.0)
		player.call("advance", 0.0)
		_check(player.call("detection_worldspace") == "Tamriel", "destination worldspace")
		_lod_contract(player, streamer, 12)
		var before: Dictionary = streamer.call("terrain_lod_stats")
		var old_position: Vector3 = player.call("position")
		var far_gate := {"file": gate.get("far_file", 0), "object": gate.get("far_object", 0)}
		_move(player, far_gate, Vector3(6 * 4096, 0, 1200))
		for _frame in range(60):
			player.call("advance", 0.0)
		var new_position: Vector3 = player.call("position")
		_check(absf(new_position.x - old_position.x) > 5 * 4096, "player did not move across cell boundaries")
		_lod_contract(player, streamer, 12)
		var after: Dictionary = streamer.call("terrain_lod_stats")
		_check(before.get("blocks", "") != after.get("blocks", ""), "LOD does not follow player cell movement")
		print("RND-19 movement: before=", before.get("blocks"), " after=", after.get("blocks"))
	_move(player, door, Vector3.ZERO)
	_check(String(player.call("gameplay_cell")) == interior, "return to interior")
	_check(streamer.call("terrain_lod_stats").is_empty(), "interior retains terrain LOD")
	_check(streamer.call("object_lod_stats").is_empty(), "interior retains object LOD")
	player.free()
	streamer.free()
	if failures == 0:
		print("RESULT: PASS: PlayerNode streams terrain LOD around Whiterun")
	else:
		printerr("RESULT: FAIL: ", failures, " RND-19 check(s) failed")
	quit(0 if failures == 0 else 1)
