extends SceneTree

## RND-19 installed-data capture. Uses the Bannered Mare load door to enter
## exterior gameplay, then checks the PlayerNode-owned terrain and object LOD.

const CELL := "WhiterunBanneredMare"
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var capture := OS.get_environment("SOE_RND19_CAPTURE_PATH")
	if data.is_empty() or capture.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: SKYRIM_DATA, capture path and host extension are required")
		quit(2)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "installed data load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source failed")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0, "Bannered Mare entry failed")
	_check(player.call("start_gameplay"), "gameplay runtime failed")

	var start_world: String = String(player.call("gameplay_cell"))
	var outside := ""
	var door: Dictionary = {}
	for candidate: Dictionary in player.call("gameplay_doors"):
		if not candidate.locked:
			door = candidate
			break
	_check(not door.is_empty(), "Bannered Mare has no unlocked exterior door")
	if not door.is_empty():
		_check(player.call("activate_reference", door.file, door.object), "exterior door activation failed")
		outside = String(player.call("gameplay_cell"))
		_check(outside != start_world, "door did not move gameplay outside")
		player.call("set_facing", PI, 0.0)
		for _frame in range(180):
			player.call("advance", 1.0 / 60.0)
			await process_frame
		var player_camera: Camera3D
		for child: Node in player.get_children():
			if child is Camera3D:
				player_camera = child
				break
		_check(player_camera != null, "PlayerNode camera is missing")
		if player_camera != null:
			player_camera.global_position += Vector3.UP * 40.0

	var terrain: Dictionary = streamer.call("terrain_lod_stats")
	var objects: Dictionary = streamer.call("object_lod_stats")
	_check(String(terrain.get("world", "")) == "Tamriel", "terrain LOD did not follow the exterior worldspace")
	_check(int(terrain.get("lod_radius", 0)) == 16, "terrain LOD did not use the configured radius")
	_check(int(terrain.get("instances", 0)) > 0, "PlayerNode loaded no rendered distant terrain")
	_check(not String(terrain.get("blocks", "")).is_empty(), "terrain LOD has no resident blocks")
	_check(String(objects.get("world", "")) == "Tamriel", "object LOD did not follow the exterior worldspace")
	_check(int(objects.get("instances", 0)) > 0, "PlayerNode loaded no rendered object LOD")

	await process_frame
	await process_frame
	var image := get_root().get_texture().get_image()
	var err := image.save_png(capture)
	_check(err == OK, "could not save exterior LOD capture")
	var background := image.get_pixel(0, 0)
	var changed_pixels := 0
	for y in range(int(image.get_height() / 3), image.get_height()):
		for x in range(image.get_width()):
			var color := image.get_pixel(x, y)
			var color_delta := absf(color.r - background.r) + absf(color.g - background.g) + absf(color.b - background.b)
			if color_delta > 0.12:
				changed_pixels += 1
	_check(changed_pixels >= 500, "captured lower view is background instead of distant terrain")

	print("RND-19 capture: exterior=", outside,
		" terrain_world=", terrain.get("world"), " terrain_blocks=", terrain.get("blocks"),
		" terrain_instances=", terrain.get("instances"), " object_instances=", objects.get("instances"),
		" non_background_lower_pixels=", changed_pixels, " capture=", capture)
	player.free()
	streamer.call("clear_terrain_lod")
	streamer.call("clear_object_lod")
	streamer.free()
	if failures == 0:
		print("RESULT: PASS: PlayerNode streams terrain LOD around Whiterun")
	else:
		printerr("RESULT: FAIL: ", failures, " RND-19 check(s) failed")
	quit(0 if failures == 0 else 1)
