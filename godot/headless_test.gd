extends SceneTree

## Headless smoke test for the soe-godot-host GDExtension (ENGINE-PLAN.md
## round 3, "Godot headless load"). Run with:
##   godot --headless --path godot --script headless_test.gd
##
## crates/godot-host now links esm, bsa and nif (docs/godot-host.md, "Real
## cell"), so when SKYRIM_DATA points at an installed game's Data folder this
## script calls CellStreamerNode.load_data and stream_cell to build real
## meshes for WhiterunBanneredMare and exits on that result alone, no frame
## wait needed since the call is synchronous. When SKYRIM_DATA is unset it
## falls back to the original synthetic run: one focus in exterior cell
## (0, 0), one placeholder MeshInstance3D per cell_load_requested signal,
## exiting 0 once at least one was built.

var frame := 0
var loaded := {}
var scene_root: Node3D
var failed := false
# Set once _run_real has already decided pass or fail and called quit, so a
# stray _process call before the tree actually stops does not run the
# synthetic path's frame-count logic against an empty loaded map.
var real_run_done := false

func _initialize() -> void:
	var skyrim_data := OS.get_environment("SKYRIM_DATA")
	print("SKYRIM_DATA=", skyrim_data)
	print("Godot version: ", Engine.get_version_info().string)

	if not ClassDB.class_exists("CellStreamerNode"):
		printerr("FAIL: CellStreamerNode is not registered, the extension did not load")
		failed = true
		quit(1)
		return

	scene_root = Node3D.new()
	scene_root.name = "Root"
	get_root().add_child(scene_root)

	if skyrim_data != "":
		_run_real(skyrim_data)
		return

	_run_synthetic()

func _run_real(skyrim_data: String) -> void:
	var streamer := ClassDB.instantiate("CellStreamerNode") as Node3D
	if streamer == null:
		printerr("FAIL: could not instantiate CellStreamerNode")
		quit(1)
		return
	streamer.name = "Streamer"
	scene_root.add_child(streamer)

	if not streamer.call("load_data", skyrim_data):
		printerr("RESULT: FAIL: load_data could not open %s" % skyrim_data)
		quit(1)
		return

	var cell_name := "WhiterunBanneredMare"
	var built: int = streamer.call("stream_cell", cell_name)
	print("Godot headless load: cell = %s, mesh nodes built = %d" % [cell_name, built])
	real_run_done = true
	if built > 0:
		print("RESULT: PASS")
		quit(0)
	else:
		printerr("RESULT: FAIL: stream_cell built no meshes for %s" % cell_name)
		quit(1)

func _run_synthetic() -> void:
	var focus := Node3D.new()
	focus.name = "Focus"
	# 29.25 m, 29.25 m puts the focus mid cell (0, 0) in Skyrim units
	# (docs/godot-host.md: one cell is 4096 units, about 58.5 m).
	focus.global_position = Vector3(29.25, 0.0, -29.25)
	scene_root.add_child(focus)

	var streamer := ClassDB.instantiate("CellStreamerNode") as Node3D
	if streamer == null:
		printerr("FAIL: could not instantiate CellStreamerNode")
		failed = true
		quit(1)
		return
	streamer.name = "Streamer"
	streamer.set("load_radius", 1)
	streamer.set("unload_radius", 2)
	streamer.set("max_loads_per_tick", 64)
	streamer.set("max_unloads_per_tick", 64)
	scene_root.add_child(streamer)
	streamer.set("focus", focus)

	streamer.connect("cell_load_requested", _on_cell_load_requested)
	streamer.call("reconfigure")

func _on_cell_load_requested(cell_x: int, cell_y: int) -> void:
	var key := "%d,%d" % [cell_x, cell_y]
	if loaded.has(key):
		return
	var mesh_instance := MeshInstance3D.new()
	mesh_instance.name = "Cell_%s" % key
	var box := BoxMesh.new()
	box.size = Vector3(58.5, 1.0, 58.5)
	mesh_instance.mesh = box
	scene_root.add_child(mesh_instance)
	loaded[key] = true

func _process(_delta: float) -> bool:
	if failed or real_run_done:
		return true
	frame += 1
	if frame < 10:
		return false
	var mesh_count := loaded.size()
	print("Godot headless load: cells streamed = %d, mesh nodes built = %d" % [loaded.size(), mesh_count])
	if mesh_count > 0:
		print("RESULT: PASS")
		quit(0)
	else:
		printerr("RESULT: FAIL: no cells were streamed after %d frames" % frame)
		quit(1)
	return true
