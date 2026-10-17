# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree

## Loaded-cell exclusion applies before the next LOD update, including empty cells.
var failures := 0


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("LOD_RESIDENCY_FAIL ", message)


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var streamer := ClassDB.instantiate("CellStreamerNode") as Node3D
	root.add_child(streamer)
	_check(streamer.call("load_data", OS.get_environment("SOE_LOD_FIXTURE")), "fixture opens")
	_check(streamer.call("stream_object_lod", "Parent", 0, 0, 2, 16, 0) > 0, "parent BTO initially drawn")
	var before: Dictionary = streamer.call("object_lod_stats")
	_check(before.get("blocks", "") == "4:8:0", "authored BTO block")
	_check(streamer.call("stream_cell", "EmptyChildCell") == 0, "empty child cell installs")
	var installed: Dictionary = streamer.call("object_lod_stats")
	_check(installed.get("instances", -1) == 0, "no BTO while the loaded cell has no visible batches")
	_check(installed.get("resident", -1) == 0, "overlapping block residency released at installation")
	_check(streamer.call("update_object_lod", 0, 0) == 0, "loaded cell excludes next update")
	_check(streamer.call("object_lod_stats").get("instances", -1) == 0, "BTO remains excluded")
	_check(streamer.call("release_exterior_cell", 8, 0) == 0, "empty cell releases without batches")
	_check(streamer.call("update_object_lod", 0, 0) > 0, "unloaded cell regains object LOD")
	print("LOD_RESIDENCY_RESULT ", "PASS" if failures == 0 else "FAIL", " before=", before, " installed=", installed)
	streamer.free()
	quit(0 if failures == 0 else 1)
