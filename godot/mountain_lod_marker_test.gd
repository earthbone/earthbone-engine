# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree

## Remote small-world markers keep parent objects until city models replace them.
var failures := 0

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("MOUNTAIN_MARKER_FAIL ", message)

func _initialize() -> void:
	call_deferred("_run")

func _lod_surfaces(streamer: Node3D) -> Array:
	if streamer.has_method("lod_band_surfaces"):
		return streamer.call("lod_band_surfaces", "object")
	var rows: Array = streamer.call("play_path_surfaces")
	rows = rows.filter(func(row: Dictionary) -> bool: return row.kind == "lod")
	for row: Dictionary in rows:
		row.triangles = row.mesh.surface_get_array_index_len(0) / 3
	return rows

func _run() -> void:
	var streamer := ClassDB.instantiate("CellStreamerNode") as Node3D
	root.add_child(streamer)
	_check(streamer.call("load_data", OS.get_environment("SOE_LOD_FIXTURE")), "fixture opens")
	_check(streamer.call("stream_object_lod", "Parent", 5, -1, 2, 32, 0) == 1, "mountain initially drawn")
	var before: Dictionary = _lod_surfaces(streamer)[0]
	_check(streamer.call("stream_cell", "RemoteMarkerCell") == 0, "remote child cell installs")
	var after: Array = _lod_surfaces(streamer)
	_check(after.size() == 1, "remote child must not remove the mountain")
	if not after.is_empty():
		_check(after[0].rid == before.rid and after[0].triangles == 2, "marker installation preserves drawable geometry")
	_check(streamer.call("stream_cell", "CityModelCell") > 0, "authored city object installs")
	var partial: Dictionary = streamer.call("object_lod_stats")
	_check(partial.get("triangles", -1) == 1, "city object hides its source cell while keeping the neighboring mountain")
	var full: Array = streamer.call("play_path_surfaces")
	full = full.filter(func(row: Dictionary) -> bool: return row.kind != "lod")
	_check(full.size() == 1, "one full model replaces the mountain")
	if not full.is_empty():
		var arrays: Array = before.mesh.surface_get_arrays(0)
		var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
		var old_bounds := AABB(vertices[0], Vector3.ZERO).expand(vertices[1]).expand(vertices[2])
		var new_bounds: AABB = full[0].bounds
		_check(old_bounds.position.is_equal_approx(new_bounds.position) and old_bounds.size.is_equal_approx(new_bounds.size), "near and medium geometry meet without displacement")
	streamer.call("release_exterior_cell", 13, -10)
	_check(streamer.call("update_object_lod", 5, -1) >= 0, "object update succeeds after retirement")
	_check(streamer.call("object_lod_stats").get("triangles", -1) == 2, "parent mountain returns after city model retires")
	var restored: Array = _lod_surfaces(streamer)
	_check(restored.size() == 1 and restored[0].triangles == before.triangles, "retirement restores complete mountain")
	print("MOUNTAIN_MARKER_RESULT ", "PASS" if failures == 0 else "FAIL")
	streamer.free()
	quit(0 if failures == 0 else 1)
