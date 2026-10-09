extends SceneTree

## RND-32 installed plains: uploaded TREE and FLOR placements and captures.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL rnd_32: ", message)

func _capture(camera: Camera3D, object: Dictionary, path: String) -> void:
	var position: Array = object.position
	var focus := Vector3(float(position[0]), float(position[2]), -float(position[1])) * 0.0142875
	var tree: bool = object.kind == "TREE"
	focus.y += 0.5
	camera.global_position = focus + (Vector3(3.0, 1.5, 3.0) if tree else Vector3(2.0, 1.0, 2.0))
	camera.look_at(focus)
	for frame in range(4):
		await process_frame
		RenderingServer.force_draw(false)
	var pixels := get_root().get_texture().get_image()
	_check(pixels.save_png(path) == OK, "capture saves")
	var foreground := 0
	for y in range(pixels.get_height()):
		for x in range(pixels.get_width()):
			var color := pixels.get_pixel(x, y)
			if max(abs(color.r - 0.15), abs(color.g - 0.2), abs(color.b - 0.3)) > 0.1:
				foreground += 1
	_check(foreground >= 100, "capture contains at least 100 scene pixels")
	print("rnd_32 capture=", path, " reference=", object.object, " foreground_pixels=", foreground)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("SKIP rnd_32: SKYRIM_DATA unset")
		quit(0)
		return
	var target := OS.get_environment("CARGO_TARGET_DIR")
	if target.is_empty():
		target = ProjectSettings.globalize_path("res://../target")
	var manifest = JSON.parse_string(FileAccess.get_file_as_string(target.path_join("rnd_32_whiterun.json")))
	if not manifest is Array:
		printerr("FAIL rnd_32: run the installed Rust survey first")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "load installed data")
	var children := streamer.get_child_count()
	var audits := {}
	var totals := {"TREE": 0, "FLOR": 0}
	for cell in manifest:
		if cell.cell == "WhiterunBanneredMare":
			continue
		_check(streamer.call("stream_exterior", cell.cell, 0) > 0, "stream " + cell.cell)
		var stats: Dictionary = streamer.call("last_stats")
		var counts: Dictionary = stats.get("reference_counts", {})
		audits[cell.cell] = counts
		for kind in ["TREE", "FLOR"]:
			totals[kind] += int(counts.get(kind, {}).get("built", 0))
		for kind in cell.counts:
			_check(counts.get(kind, {}).get("built", -1) == cell.counts[kind].built, "host built count matches CPU " + cell.cell + " " + kind)
	_check(streamer.get_child_count() == children, "placed models add no scene nodes")
	var albedo: Dictionary = streamer.call("material_albedo_stats")
	_check(int(albedo.get("checker", -1)) == 0, "audited plains have no checker albedo bindings")
	for kind in ["TREE", "FLOR"]:
		_check(totals[kind] > 0, kind + " has built placements")
	var selected := {}
	var checked := 0
	for cell in manifest:
		if cell.cell == "WhiterunBanneredMare":
			continue
		for object in cell.objects:
			_check(streamer.call("rendered_reference_count", int(object.file), int(object.object)) > 0, "uploaded " + object.kind + " " + str(object.object))
			checked += 1
			if not selected.has(object.kind):
				selected[object.kind] = object
	_check(checked > 0, "manifest contains placed plants")
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color(0.15, 0.2, 0.3)
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color.WHITE
	environment.environment.ambient_light_energy = 0.8
	get_root().add_child(environment)
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-45.0, -30.0, 0.0)
	get_root().add_child(sun)
	var camera := Camera3D.new()
	get_root().add_child(camera)
	camera.current = true
	var directory := target.path_join("rnd_32")
	_check(DirAccess.make_dir_recursive_absolute(directory) == OK, "capture directory")
	for kind in ["TREE", "FLOR"]:
		_check(selected.has(kind), "manifest names a " + kind)
		if selected.has(kind):
			await _capture(camera, selected[kind], directory.path_join(kind.to_lower() + ".png"))
	print("rnd_32 checked_placements=", checked, " albedo=", albedo, " counts=", audits)
	streamer.call("clear")
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " rnd_32_windowed_plains_show_placed_trees_and_flora")
	quit(0 if failures == 0 else 1)
