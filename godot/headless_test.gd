extends SceneTree

## Headless smoke test for the soe-godot-host GDExtension (ENGINE-PLAN.md
## round 3, "Godot headless load"). Run with:
##   godot --headless --path godot --script headless_test.gd
##
## What this actually exercises: crates/godot-host has no esm, bsa or nif
## dependency yet (see docs/godot-host.md, "Round 2 hooks"), so the
## extension cannot resolve a named cell such as WhiterunBanneredMare or
## read anything under SKYRIM_DATA. It only exposes CellStreamerNode, the
## coordinate-based load and unload decision logic, plus two Rust-only
## conversion functions that GDScript cannot call directly. This script
## drives CellStreamerNode with one synthetic focus placed in exterior cell
## (0, 0), lets it emit cell_load_requested, builds one placeholder
## MeshInstance3D per requested cell, and prints the resulting count. It
## exits 0 when at least one cell streamed and a mesh node was built for
## it, 1 otherwise.

var frame := 0
var loaded := {}
var scene_root: Node3D
var failed := false

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
	if failed:
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
