extends SceneTree

## RND-28 gameplay scenery handoff, world changes and an installed mountain silhouette.
const CELL := "WhiterunBanneredMare"
const MOUNTAIN_SKY_PIXEL_THRESHOLD := 500
const TREE_PIXEL_THRESHOLD := 25
var failures := 0
var cells := {}

func _initialize() -> void:
	call_deferred("rnd_28_player_lod_capture")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _lod_contract(player: Node3D, streamer: Node3D, radius: int) -> void:
	var position: Vector3 = player.call("position")
	var focus := Vector2i(floori(position.x / 4096.0), floori(position.y / 4096.0))
	var near: Dictionary = player.call("exterior_resources")
	_check(not near.is_empty(), "no full-resolution exterior cells")
	for kind in ["terrain", "object", "tree"]:
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
			if kind != "terrain":
				for id in near:
					if cells.has(id):
						var coord: Vector2i = cells[id]
						_check(not (coord.x >= origin.x and coord.x < origin.x + span and coord.y >= origin.y and coord.y < origin.y + span), kind + " block covers full cell " + id)

func _move(player: Node3D, reference: Dictionary, offset: Vector3) -> void:
	_check(player.call("queue_player_input", {"action": "move_to", "file": reference.file, "object": reference.object, "offset": offset, "match_rotation": false}), "queue player movement")
	_check(player.call("step_player_input"), "apply player movement")
	player.call("advance", 0.0)

func rnd_28_player_lod_capture() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var capture := OS.get_environment("SOE_RND28_CAPTURE_PATH")
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
	_lod_contract(player, streamer, 96)
	for _frame in range(45):
		player.call("advance", 1.0 / 60.0)
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "Whiterun gate")
	if not gate.is_empty():
		_check(player.call("activate_reference", gate.file, gate.object), "cross to Tamriel")
		player.call("advance", 0.0)
		_check(player.call("detection_worldspace") == "Tamriel", "destination worldspace")
		var far_gate := {"file": gate.get("far_file", 0), "object": gate.get("far_object", 0)}
		_move(player, far_gate, Vector3(-4096, -4096, 500))
		for _frame in range(45):
			player.call("advance", 1.0 / 60.0)
		_check(not player.call("door_fade_active"), "exterior door fade did not finish")
		for _frame in range(90):
			player.call("advance", 0.0)
		_lod_contract(player, streamer, 96)
		var object_stats: Dictionary = streamer.call("object_lod_stats")
		var rendered_levels := {}
		for entry in String(object_stats.get("blocks", "")).split(" ", false):
			rendered_levels[int(entry.split(":")[0])] = true
		for level in [4, 8, 16]:
			_check(rendered_levels.has(level), "missing installed object LOD level " + str(level))
		# CONTEXT: Flat sky identifies the mountain silhouette above terrain in the control.
		var environment := Environment.new()
		environment.background_mode = Environment.BG_COLOR
		environment.background_color = Color(0.15, 0.25, 0.45)
		environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
		environment.ambient_light_color = Color.WHITE
		environment.ambient_light_energy = 0.8
		root.world_3d.environment = environment
		var peak_values := OS.get_environment("SOE_RND28_MOUNTAIN_TARGET").split(",")
		_check(peak_values.size() == 3, "Throat of the World peak target")
		if peak_values.size() == 3:
			var peak := Vector3(float(peak_values[0]), float(peak_values[1]), float(peak_values[2]))
			var position: Vector3 = player.call("position")
			var delta := peak - position
			var yaw := atan2(delta.x, delta.y)
			var pitch := atan2(delta.z, Vector2(delta.x, delta.y).length()) * 0.6
			player.call("set_facing", yaw, pitch)
			print("RND-28 view: player=", position, " peak=", peak, " yaw=", yaw, " pitch=", pitch)
		for _frame in range(10):
			await process_frame
		streamer.call("set_object_lod_visible", false)
		streamer.call("set_tree_lod_visible", false)
		await process_frame
		await process_frame
		var control := root.get_texture().get_image()
		streamer.call("set_object_lod_visible", true)
		await process_frame
		await process_frame
		var objects := root.get_texture().get_image()
		streamer.call("set_tree_lod_visible", true)
		await process_frame
		await process_frame
		var image := root.get_texture().get_image()
		_check(image.save_png(capture) == OK, "save mountain capture")
		_check(control.save_png(capture.trim_suffix(".png") + "-terrain-only.png") == OK, "save terrain-only control")
		_check(objects.save_png(capture.trim_suffix(".png") + "-objects.png") == OK, "save object silhouette")
		var sky := control.get_pixel(0, 0)
		var mountain_sky_pixels := 0
		var tree_pixels := 0
		for y in range(30, 180):
			for x in range(80, 560):
				var a := objects.get_pixel(x, y)
				var b := control.get_pixel(x, y)
				var sky_distance := absf(sky.r-b.r) + absf(sky.g-b.g) + absf(sky.b-b.b)
				var difference := absf(a.r-b.r) + absf(a.g-b.g) + absf(a.b-b.b)
				if sky_distance < 0.03 and difference > 0.12:
					mountain_sky_pixels += 1
		for y in range(image.get_height()):
			for x in range(image.get_width()):
				var a := image.get_pixel(x, y)
				var b := objects.get_pixel(x, y)
				if absf(a.r-b.r) + absf(a.g-b.g) + absf(a.b-b.b) > 0.08:
					tree_pixels += 1
		_check(mountain_sky_pixels >= MOUNTAIN_SKY_PIXEL_THRESHOLD, "mountain silhouette does not cover sky left by terrain alone")
		_check(tree_pixels >= TREE_PIXEL_THRESHOLD, "tree LOD has no measurable forest contribution")
		print("RND-28 silhouette: mountain_sky_pixels=", mountain_sky_pixels, " threshold=", MOUNTAIN_SKY_PIXEL_THRESHOLD, " tree_pixels=", tree_pixels, " capture=", capture)
		print("RND-28 object blocks: ", object_stats.get("blocks"))
		var before := {}
		for kind in ["terrain", "object", "tree"]:
			before[kind] = streamer.call(kind + "_lod_stats").get("blocks", "")
		_move(player, far_gate, Vector3(6 * 4096, 0, 500))
		for _frame in range(90):
			player.call("advance", 0.0)
		_lod_contract(player, streamer, 96)
		for kind in ["terrain", "object", "tree"]:
			_check(before[kind] != streamer.call(kind + "_lod_stats").get("blocks", ""), kind + " LOD does not follow movement")

	var other_id := OS.get_environment("SOE_RND28_OTHER_REFERENCE").split(":")
	_check(other_id.size() == 2, "Solstheim movement target")
	if other_id.size() == 2:
		_move(player, {"file": int(other_id[0]), "object": int(other_id[1])}, Vector3(0, 0, 500))
		for _frame in range(90):
			player.call("advance", 0.0)
		_check(player.call("detection_worldspace") == "DLC2SolstheimWorld", "Solstheim worldspace switch")
		for kind in ["terrain", "object", "tree"]:
			var stats: Dictionary = streamer.call(kind + "_lod_stats")
			_check(stats.get("world", "") == "DLC2SolstheimWorld", kind + " keeps previous world's LOD")
			_check(int(stats.get("instances", 0)) > 0, kind + " missing Solstheim instances")
			_check(int(stats.get("failed", -1)) == 0, kind + " Solstheim read failures")

	_move(player, door, Vector3.ZERO)
	_check(String(player.call("gameplay_cell")) == interior, "return to interior")
	_check(streamer.call("terrain_lod_stats").is_empty(), "interior retains terrain LOD")
	_check(streamer.call("object_lod_stats").is_empty(), "interior retains object LOD")
	_check(streamer.call("tree_lod_stats").is_empty(), "interior retains tree LOD")
	player.free()
	streamer.free()
	if failures == 0:
		print("RESULT: PASS: PlayerNode streams object and tree LOD around Whiterun")
	else:
		printerr("RESULT: FAIL: ", failures, " RND-28 check(s) failed")
	quit(0 if failures == 0 else 1)
