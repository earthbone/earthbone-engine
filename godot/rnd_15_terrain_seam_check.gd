extends SceneTree

## RND-15's done check, exercised from a live Godot process against the
## compiled soe-godot-host extension: the WhiterunOrigin radius 1 block
## streams with land on all 9 cells and occupies one contiguous 3 by 3
## footprint, the structural precondition for the seam gap this check
## verifies against in crates/godot-host/tests/terrain_seam_reconciliation_check.rs.
##
## Budget: headless import plus one stream_exterior call, expected under
## 30 seconds against installed game data, well inside the 20 minute
## installed-data timeout.
##
## This script cannot re-run the Rust test's own per-vertex edge
## comparison: soe-godot-host draws each cell's terrain as one bare
## RenderingServer instance (crates/godot-host/src/godot_layer.rs,
## add_instance), never a MeshInstance3D node, and Godot 4's
## RenderingServer has no instance_get_base to recover a mesh RID from an
## instance RID (only the setter, instance_set_base), so nothing already
## exposed to GDScript can read the uploaded vertex positions back. The
## numeric "no shared edge disagrees by more than one sample" clause is
## covered instead by
## crates/godot-host/tests/terrain_seam_reconciliation_check.rs, which
## reads DataSource::stream_exterior's own SurfaceArrays directly in Rust.
## This script is the structural half of the same done check, run through
## the actual compiled extension a live game session loads.
##
## Environment variables:
##   SKYRIM_DATA   installed game Data folder (required)
##
## Run headless:
## Godot_v4.7.2-stable_linux.x86_64 --headless --path <project> --script rnd_15_terrain_seam_check.gd

var failures := 0


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("RESULT: FAIL: ", message)


func _initialize() -> void:
	call_deferred("_run")


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

	var built: int = streamer.call("stream_exterior", "WhiterunOrigin", 1)
	_check(built > 0, "stream_exterior(WhiterunOrigin, 1) built nothing")

	var stats: Dictionary = streamer.call("last_stats")
	# RND-15's own premise: some of the block's 9 cells carry no LAND of
	# their own and fall back to the parent worldspace, which is where the
	# seam mismatch came from. land_cells counts every cell that resolved
	# LAND at all, own or inherited, so it must cover the whole block.
	_check(stats.get("land_cells", -1) == 9,
		"expected land on all 9 WhiterunOrigin block cells, last_stats reports %s" % stats.get("land_cells"))
	# last_stats packs cell_list as a single space-separated "(x,y)" string,
	# not an array (see CellStreamerNode::to_dictionary), so split it here.
	var cell_list: PackedStringArray = String(stats.get("cell_list", "")).split(" ", false)
	_check(cell_list.size() == 9,
		"expected 9 cells in the block, last_stats.cell_list has %d" % cell_list.size())

	# The block is a contiguous 3 by 3 grid of cells (radius 1), so its
	# streamed bounds should span about 3 cell widths on each horizontal
	# axis. CELL_METRES here mirrors crates/godot-host/src/units.rs
	# (CELL_UNITS 4096.0 times METRES_PER_UNIT 0.0142875), a fixed game
	# constant, not the code under test.
	var cell_metres := 4096.0 * 0.0142875
	var bounds: AABB = streamer.call("scene_bounds")
	var span_x := bounds.size.x
	var span_z := bounds.size.z
	var expect := cell_metres * 3.0
	var tol := cell_metres * 0.5
	_check(absf(span_x - expect) < tol,
		"block x span %.2f m is not close to the expected 3 cell widths (%.2f m)" % [span_x, expect])
	_check(absf(span_z - expect) < tol,
		"block z span %.2f m is not close to the expected 3 cell widths (%.2f m)" % [span_z, expect])

	print("RND-15 SMOKE: built=", built, " land_cells=", stats.get("land_cells"),
		" span_x=", span_x, " span_z=", span_z, " failures=", failures)
	if failures == 0:
		print("RESULT: PASS: WhiterunOrigin radius 1 streams land on all 9 cells in one contiguous block")
	else:
		printerr("RESULT: FAIL: %d check(s) failed, see above" % failures)
	streamer.free()
	quit(0 if failures == 0 else 1)
