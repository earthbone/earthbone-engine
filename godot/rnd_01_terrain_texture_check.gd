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
	var checked := false
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
		var colors := {}
		for point in [Vector2i(0, 0), Vector2i(127, 63), Vector2i(128, 127), Vector2i(255, 255)]:
			var color := image.get_pixel(point.x, point.y)
			colors[color.to_html(false)] = true
		_check(colors.size() > 1, "terrain albedo contains blended texture colors")
		_check(colors.keys().any(func(color: String) -> bool: return color != "ffffff"),
			"terrain albedo is not white")
		checked = true
		break
	_check(checked, "a terrain surface has an uploaded albedo texture")
	streamer.free()
	if failures == 0:
		print("RESULT: PASS: vanilla exterior terrain draws its blended texture")
		quit(0)
	else:
		printerr("RESULT: FAIL: %d check(s) failed" % failures)
		quit(1)
