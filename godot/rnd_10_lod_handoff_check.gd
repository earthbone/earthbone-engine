extends SceneTree

## RND-10 live renderer check. It streams a full-resolution exterior cell,
## then real BTR terrain and BTO object LOD around that block's focus. Each
## path must create RenderingServer instances, and every resident LOD block
## must be spatially disjoint from the streamed near cells.

var failures := 0

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _initialize() -> void:
	call_deferred("_run")

func _blocks_are_disjoint(encoded: String, cells: Array[Vector2i]) -> bool:
	for entry in encoded.split(" ", false):
		var values := entry.split(":")
		if values.size() != 3:
			return false
		var span := int(values[0])
		var x := int(values[1])
		var y := int(values[2])
		for cell in cells:
			if cell.x >= x and cell.x < x + span and cell.y >= y and cell.y < y + span:
				return false
	return true

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		printerr("FAIL: SKYRIM_DATA is required")
		quit(2)
		return
	if not ClassDB.class_exists("CellStreamerNode"):
		printerr("FAIL: soe-godot-host extension is not loaded")
		quit(2)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "load_data failed")
	if failures > 0:
		streamer.free()
		quit(1)
		return

	var selected_cell: Dictionary = {}
	var selected_distance := 2147483647
	for cell in streamer.call("editor_cell_tree"):
		if bool(cell.get("interior", true)) or cell.get("world_name", "") != "Tamriel":
			continue
		var distance: int = max(abs(int(cell.get("x", 0)) - 28), abs(int(cell.get("y", 0)) + 60))
		if distance < selected_distance:
			selected_cell = cell
			selected_distance = distance
	_check(not selected_cell.is_empty(), "installed data has no exterior Tamriel cells")
	if selected_cell.is_empty():
		streamer.free()
		quit(1)
		return

	var near_instances: int = streamer.call("open_cell_id", String(selected_cell.get("id", "")))
	var near_cells: Array[Vector2i] = [Vector2i(int(selected_cell.get("x", 0)), int(selected_cell.get("y", 0)))]
	_check(near_instances > 0, "full-resolution exterior cell drew no renderer instances")
	_check(not bool(selected_cell.get("interior", true)), "selected near cell has no exterior grid identity")
	var focus := near_cells[0]
	var world := String(selected_cell.get("world_name", ""))
	_check(not world.is_empty(), "selected near cell has no worldspace identity")

	var terrain_count: int = streamer.call("stream_terrain_lod", world, focus.x, focus.y, 0, 16, 0)
	var terrain: Dictionary = streamer.call("terrain_lod_stats")
	_check(terrain_count > 0, "no real terrain LOD block became resident")
	_check(int(terrain.get("instances", 0)) > 0, "terrain LOD created no RenderingServer instances")
	_check(_blocks_are_disjoint(String(terrain.get("blocks", "")), near_cells),
		"a terrain LOD block covers a full-resolution cell, so both levels would draw it")

	var object_count: int = streamer.call("stream_object_lod", world, focus.x, focus.y, 0, 16, 0)
	var objects: Dictionary = streamer.call("object_lod_stats")
	_check(object_count > 0, "no real object LOD block became resident")
	_check(int(objects.get("instances", 0)) > 0, "object LOD created no RenderingServer instances")
	_check(_blocks_are_disjoint(String(objects.get("blocks", "")), near_cells),
		"an object LOD block covers a full-resolution cell, so both levels would draw it")

	print("RND-10 renderer: focus=", focus, " nearest_BTO_distance=", selected_distance,
		" near_instances=", near_instances,
		" terrain_instances=", terrain.get("instances", 0),
		" object_instances=", objects.get("instances", 0),
		" near_cells=", near_cells.size(), " failures=", failures)
	streamer.call("clear_terrain_lod")
	streamer.call("clear_object_lod")
	streamer.free()
	if failures == 0:
		print("RESULT: PASS: terrain and object LOD hand off beyond full-resolution cells without double drawing")
	else:
		printerr("RESULT: FAIL: ", failures, " handoff check(s) failed")
	quit(0 if failures == 0 else 1)
