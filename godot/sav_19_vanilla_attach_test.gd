extends SceneTree

## Attach an atomically recovered ESS generation at its saved pose and render.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var expected: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(
		OS.get_environment("SAV_19_EXPECTED")))
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "data did not load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("auto_step", false)
	player.set("capture_mouse", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "data sharing failed")
	var attached: Dictionary = player.call("attach_restored_generation",
		OS.get_environment("SAV_19_SAVE_DIR"), 2)
	_check(attached.get("ok", false) and attached.get("state_loaded", false), str(attached))
	if not attached.get("ok", false):
		player.free()
		streamer.free()
		quit(1)
		return
	_check(player.call("gameplay_cell") == expected.cell, "saved cell differs")
	var expected_position := Vector3(expected.position[0], expected.position[1], expected.position[2])
	var position: Vector3 = player.call("position")
	_check(position.distance_to(expected_position) < 0.1, "saved position differs")
	var facing: Vector2 = player.call("facing")
	_check(abs(angle_difference(facing.x, expected.rotation[2])) < 0.001
		and abs(facing.y + expected.rotation[0]) < 0.001, "saved camera facing differs")
	var camera: Camera3D
	for child in player.get_children():
		if child is Camera3D:
			camera = child
	_check(camera != null and camera.is_current(), "camera is not current")
	_check(player.call("gameplay_days") > expected.days, "Game frame did not advance")
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.03, 0.02, 0.01)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color.WHITE
	environment.ambient_light_energy = 0.8
	var lighting := WorldEnvironment.new()
	lighting.environment = environment
	get_root().add_child(lighting)
	await process_frame
	await RenderingServer.frame_post_draw
	var image := get_root().get_texture().get_image()
	_check(not image.is_empty() and image.get_width() == 640 and image.get_height() == 360,
		"rendered frame dimensions differ")
	var distinct_pixels := 0
	var background := image.get_pixel(0, 0)
	for y in range(0, image.get_height(), 2):
		for x in range(0, image.get_width(), 2):
			var color := image.get_pixel(x, y)
			if Vector3(color.r, color.g, color.b).distance_to(
				Vector3(background.r, background.g, background.b)) > 0.05:
				distinct_pixels += 1
	_check(distinct_pixels >= 100, "fewer than 100 scene samples differ from background")
	_check(image.save_png(OS.get_environment("SAV_19_CAPTURE")) == OK, "capture write failed")
	print("SAV-19 save=", expected.save, " cell=", expected.cell, " scene_samples=", distinct_pixels)
	player.free()
	streamer.free()
	lighting.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " sav_19 vanilla attach")
	quit(0 if failures == 0 else 1)
