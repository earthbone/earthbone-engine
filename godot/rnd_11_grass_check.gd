extends SceneTree

## RND-11's done check, run from a live Godot process against the compiled
## soe-godot-host extension: the WhiterunOrigin radius 1 block streams
## record-backed grass placements, the count stays inside the per cell
## budget, a second stream of the same block places the same count, and
## releasing every cell of the block frees its draws exactly once.
##
## The per-placement transform comparison and the MultiMesh batch shape are
## checked in crates/godot-host/src/data_source/grass_tests.rs against a
## synthetic LAND fixture, since GDScript cannot read RenderingServer
## instance transforms back.
##
## Environment variables:
##   SKYRIM_DATA   installed game Data folder (required)
##
## Run headless:
## Godot_v4.7.2-stable_linux.x86_64 --headless --path <project> --script rnd_11_grass_check.gd

## Mirrors MAX_GRASS_PLACEMENTS_PER_CELL in crates/godot-host/src/data_source/grass.rs.
const MAX_PER_CELL := 512

var failures := 0


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("RESULT: FAIL: ", message)


func _initialize() -> void:
	call_deferred("_run")


func _stream(streamer: Node3D) -> Dictionary:
	var built: int = streamer.call("stream_exterior", "WhiterunOrigin", 1)
	_check(built > 0, "stream_exterior(WhiterunOrigin, 1) built nothing")
	return streamer.call("last_stats")


func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		printerr("RESULT: FAIL: SKYRIM_DATA is required")
		quit(2)
		return
	if not ClassDB.class_exists("CellStreamerNode"):
		printerr("RESULT: FAIL: soe-godot-host extension did not load (CellStreamerNode missing)")
		quit(2)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "load_data could not open %s" % data)
	if failures > 0:
		streamer.free()
		quit(1)
		return

	var first := _stream(streamer)
	var placed: int = first.get("grass_placements", -1)
	_check(placed > 0, "no grass placed around WhiterunOrigin, last_stats reports %s" % placed)
	_check(placed <= 9 * MAX_PER_CELL,
		"grass placements %d exceed the 9 cell budget %d" % [placed, 9 * MAX_PER_CELL])

	streamer.call("clear")
	var second := _stream(streamer)
	_check(second.get("grass_placements", -2) == placed,
		"second stream placed %s grass instances, first placed %d" % [second.get("grass_placements"), placed])

	var cells: PackedStringArray = String(second.get("cell_list", "")).split(" ", false)
	_check(cells.size() == 9, "expected 9 cells, cell_list has %d" % cells.size())
	var released := 0
	var again := 0
	for entry in cells:
		var xy := entry.trim_prefix("(").trim_suffix(")").split(",")
		var x := int(xy[0])
		var y := int(xy[1])
		released += int(streamer.call("release_exterior_cell", x, y))
		again += int(streamer.call("release_exterior_cell", x, y))
	_check(released > 0, "releasing the block freed no instances")
	_check(again == 0, "a second release freed %d instances, expected none" % again)

	print("RND-11 SMOKE: grass_placements=", placed, " released=", released,
		" failures=", failures)
	if failures == 0:
		print("RESULT: PASS: WhiterunOrigin block places bounded, repeatable grass and releases it on unload")
	else:
		printerr("RESULT: FAIL: %d check(s) failed, see above" % failures)
	streamer.free()
	quit(0 if failures == 0 else 1)
