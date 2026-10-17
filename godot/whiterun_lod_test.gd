# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## RND-42 checks the submitted scenery at the Plains District and city walls.
const CELL_METRES := 4096.0 * 0.0142875
var failures := 0
var reference_cells: Dictionary
var exterior_cells: Dictionary
var full_lod_references := {}
var placed_lod_rids: Array[RID] = []
var last_checked_coverage := ""
var masked_full_lod_members := 0


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", true)


func _process(_delta: float) -> bool:
	return false


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("WHITERUN_LOD_FAIL ", message)


func _settle_location() -> void:
	var frames := 0
	while (player.call("is_load_door_pending") or player.call("is_location_preparing")) and frames < 1800:
		player.call("advance", 1.0 / 60.0)
		_check_handoff()
		await process_frame
		frames += 1
	_check(player.call("gameplay_location_ready"), "location ready")
	# CONTEXT: Keep each authored probe position fixed after the location handoff.
	player.set("auto_step", false)
	for _frame in range(90):
		player.call("advance", 0.0)
		_check_handoff()
		await process_frame


func _array_member_is_hidden(surface: Dictionary, instance: int, reference: String) -> bool:
	var mesh: ArrayMesh = surface.mesh
	var arrays := mesh.surface_get_arrays(0)
	var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	var indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
	var single_reference := true
	for member: String in surface.get("references", PackedStringArray()):
		single_reference = single_reference and member == reference
	var checked := 0
	for triangle in range(indices.size() / 3):
		if not single_reference and streamer.call("reference_at", instance, triangle) != reference:
			continue
		checked += 1
		var a := vertices[indices[triangle * 3]]
		var b := vertices[indices[triangle * 3 + 1]]
		var c := vertices[indices[triangle * 3 + 2]]
		if (b - a).cross(c - a) != Vector3.ZERO:
			return false
	return checked > 0


func _check_handoff(force: bool = false) -> void:
	var covered: String = streamer.call("terrain_lod_stats").get("covered_cells", "")
	if covered == last_checked_coverage and not force:
		return
	last_checked_coverage = covered
	var full := {}
	var distant := {}
	var near_references := {}
	for reference: String in streamer.call("distant_references"):
		distant[reference] = true
	for surface: Dictionary in streamer.call("play_path_surfaces"):
		if surface.get("pending_distant", false):
			continue
		var owner: String = surface.get("cell", "")
		if not exterior_cells.has(owner):
			continue
		var coordinate: Array = exterior_cells[owner]
		full[Vector2i(int(coordinate[0]), int(coordinate[1]))] = true
		for reference: String in surface.get("references", PackedStringArray()):
			near_references[reference] = true
			_check(not distant.has(reference), "handoff duplicates Full LOD reference " + reference)
	var surfaces: Array = streamer.call("play_path_surfaces")
	for instance in range(surfaces.size()):
		var surface: Dictionary = surfaces[instance]
		if surface.get("pending_distant", false):
			continue
		if exterior_cells.has(surface.get("cell", "")):
			continue
		var references: PackedStringArray = surface.get("references", PackedStringArray())
		for index in range(references.size()):
			if not full_lod_references.has(references[index]) or not near_references.has(references[index]):
				continue
			var multi: MultiMesh = surface.get("multimesh")
			var masked := is_zero_approx(multi.get_instance_transform(index).basis.determinant()) if multi != null else _array_member_is_hidden(surface, instance, references[index])
			_check(masked, "old Full LOD member remains drawable " + references[index] + " multimesh=" + str(multi != null))
			if masked:
				masked_full_lod_members += 1
	var duplicates := 0
	for kind in ["object_lod_stats", "tree_lod_stats"]:
		var stats: Dictionary = streamer.call(kind)
		for text: String in String(stats.get("blocks", "")).split(" ", false):
			var parts := text.split(":")
			var span := int(parts[0])
			var x := int(parts[1])
			var y := int(parts[2])
			for cell: Vector2i in full:
				duplicates += int(cell.x >= x and cell.x < x + span and cell.y >= y and cell.y < y + span)
	_check(duplicates == 0, "handoff duplicate LOD blocks=" + str(duplicates))
	print("WHITERUN_LOD_HANDOFF full_cells=", full.size(), " duplicate_blocks=", duplicates)


func _land_height(vertices: PackedVector3Array, point: Vector3, cell: Vector2i) -> float:
	var col := clampf((point.x / CELL_METRES - cell.x) * 32.0, 0.0, 32.0)
	var row := clampf((-point.z / CELL_METRES - cell.y) * 32.0, 0.0, 32.0)
	var c := mini(floori(col), 31)
	var r := mini(floori(row), 31)
	var u := col - c
	var v := row - r
	var sw := vertices[r * 33 + c].y
	var se := vertices[r * 33 + c + 1].y
	var nw := vertices[(r + 1) * 33 + c].y
	var ne := vertices[(r + 1) * 33 + c + 1].y
	return sw + u * (se - sw) + v * (ne - se) if u >= v else sw + u * (ne - nw) + v * (nw - sw)


func _coverage(label: String) -> void:
	var near := {}
	for mesh: ArrayMesh in streamer.call("loaded_terrain_meshes"):
		var bounds := mesh.get_aabb()
		var cell := Vector2i(roundi(bounds.position.x / CELL_METRES), roundi(-bounds.end.z / CELL_METRES))
		near[cell] = mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX]
	_check(near.size() >= 25, label + " uploaded LAND coverage")
	var triangles := 0
	var overlap := 0
	var above := 0
	var highest := 0.0
	for mesh: ArrayMesh in streamer.call("terrain_lod_meshes"):
		var arrays := mesh.surface_get_arrays(0)
		var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
		var indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
		for i in range(0, indices.size(), 3):
			triangles += 1
			var a := vertices[indices[i]]
			var b := vertices[indices[i + 1]]
			var c := vertices[indices[i + 2]]
			var polygon := PackedVector2Array([Vector2(a.x, -a.z), Vector2(b.x, -b.z), Vector2(c.x, -c.z)])
			var low := Vector2i(floori(minf(a.x, minf(b.x, c.x)) / CELL_METRES), floori(minf(-a.z, minf(-b.z, -c.z)) / CELL_METRES))
			var high := Vector2i(floori(maxf(a.x, maxf(b.x, c.x)) / CELL_METRES), floori(maxf(-a.z, maxf(-b.z, -c.z)) / CELL_METRES))
			for y in range(low.y, high.y + 1):
				for x in range(low.x, high.x + 1):
					var cell := Vector2i(x, y)
					if not near.has(cell):
						continue
					# CONTEXT: Shrink by a millimetre to exclude shared boundary edges.
					var sw := Vector2(x, y) * CELL_METRES + Vector2.ONE * 0.001
					var ne := Vector2(x + 1, y + 1) * CELL_METRES - Vector2.ONE * 0.001
					var square := PackedVector2Array([sw, Vector2(ne.x, sw.y), ne, Vector2(sw.x, ne.y)])
					var intersections := Geometry2D.intersect_polygons(polygon, square)
					for intersection: PackedVector2Array in intersections:
						if intersection.size() < 3:
							continue
						overlap += 1
						var ab := polygon[1] - polygon[0]
						var ac := polygon[2] - polygon[0]
						var determinant := ab.cross(ac)
						if absf(determinant) < 0.000001:
							continue
						for sample: Vector2 in intersection:
							var relative := sample - polygon[0]
							var u := relative.cross(ac) / determinant
							var v := ab.cross(relative) / determinant
							var point := Vector3(sample.x, a.y + u * (b.y - a.y) + v * (c.y - a.y), -sample.y)
							var excess := point.y - _land_height(near[cell], point, cell)
							if excess > 0.001:
								above += 1
								highest = maxf(highest, excess)
	_check(triangles > 0, label + " distant terrain drawn")
	_check(overlap == 0 and above == 0, label + " terrain overlap=" + str(overlap) + " above=" + str(above))
	var drawn := {}
	var duplicate_references := {}
	var distant := {}
	for reference: String in streamer.call("distant_references"):
		distant[reference] = true
	for surface: Dictionary in streamer.call("play_path_surfaces"):
		if surface.get("pending_distant", false):
			continue
		if surface.kind == "lod":
			continue
		for reference: String in surface.get("references", PackedStringArray()):
			if full_lod_references.has(reference) and not exterior_cells.has(surface.get("cell", "")):
				continue
			if exterior_cells.has(surface.get("cell", "")) and distant.has(reference):
				duplicate_references[reference] = true
			if reference_cells.has(reference) and not distant.has(reference):
				var coordinate: Array = reference_cells[reference]
				drawn[Vector2i(int(coordinate[0]), int(coordinate[1]))] = true
	var duplicate_blocks := 0
	var objects: Dictionary = streamer.call("object_lod_stats")
	for text: String in String(objects.get("blocks", "")).split(" ", false):
		var parts := text.split(":")
		var span := int(parts[0])
		var x := int(parts[1])
		var y := int(parts[2])
		for cell: Vector2i in drawn:
			if cell.x >= x and cell.x < x + span and cell.y >= y and cell.y < y + span:
				duplicate_blocks += 1
	var object_intrusions := 0
	var intrusive_cells := {}
	for surface: Dictionary in streamer.call("play_path_surfaces"):
		if surface.get("pending_distant", false):
			continue
		if surface.kind != "lod":
			continue
		var mesh: ArrayMesh = surface.mesh
		var arrays := mesh.surface_get_arrays(0)
		var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
		var indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
		for i in range(0, indices.size(), 3):
			var point := (vertices[indices[i]] + vertices[indices[i + 1]] + vertices[indices[i + 2]]) / 3.0
			var cell := Vector2i(floori(point.x / CELL_METRES), floori(-point.z / CELL_METRES))
			if drawn.has(cell):
				object_intrusions += 1
				intrusive_cells[cell] = int(intrusive_cells.get(cell, 0)) + 1
	placed_lod_rids.clear()
	for surface: Dictionary in streamer.call("nif_surfaces"):
		var models := str(surface.get("models", "")).to_lower()
		if models.contains("lod") and (models.contains("whiterun") or models.contains("mountain")):
			placed_lod_rids.append(surface.rid)
			print("WHITERUN_LOD_PLACED ", label, " references=", surface.get("references", []), " models=", models)
	print("WHITERUN_LOD_OBJECT_EXTENTS ", label, " intrusions=", object_intrusions, " cells=", intrusive_cells, " instances=", objects.get("instances", 0), " triangles=", objects.get("triangles", 0))
	_check(int(objects.get("instances", 0)) > 0, label + " distant object LOD drawn")
	_check(drawn.size() > 0, label + " full-detail references drawn")
	_check(duplicate_blocks == 0, label + " duplicate object LOD blocks=" + str(duplicate_blocks))
	_check(duplicate_references.is_empty(), label + " duplicate resident Full LOD references=" + str(duplicate_references))
	print("WHITERUN_LOD_COVERAGE ", label, " position=", player.call("position"), " land=", near.size(), " triangles=", triangles, " overlap=", overlap, " above=", above, " highest_metres=", highest, " drawn_reference_cells=", drawn.size(), " duplicate_blocks=", duplicate_blocks, " terrain_mask=", streamer.call("terrain_lod_stats").get("covered_cells", ""))


func _captures(label: String) -> void:
	for overlay in root.find_children("*", "CanvasLayer", true, false):
		overlay.hide()
	var camera := root.get_camera_3d()
	var original := camera.global_transform
	if label == "pond" or label == "walls":
		var target := camera.global_position
		camera.global_position += Vector3(0, 12, 14) if label == "pond" else Vector3(0, 8, -12)
		camera.look_at(target, Vector3.UP)
	var control_meshes: Array[ArrayMesh] = []
	var controls: Array[RID] = []
	var restored_triangles := 0
	for mesh: ArrayMesh in streamer.call("loaded_terrain_meshes"):
		var arrays := mesh.surface_get_arrays(0)
		var present := {}
		var original_indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
		for i in range(0, original_indices.size() - 5, 6):
			present[original_indices[i]] = true
		var missing := PackedInt32Array()
		for row in range(32):
			for col in range(32):
				var sw := row * 33 + col
				if not present.has(sw):
					missing.append_array(PackedInt32Array([sw, sw + 1, sw + 34, sw, sw + 34, sw + 33]))
		if missing.is_empty():
			continue
		restored_triangles += missing.size() / 3
		arrays[Mesh.ARRAY_INDEX] = missing
		var control := ArrayMesh.new()
		control.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
		control.surface_set_material(0, mesh.surface_get_material(0))
		control_meshes.append(control)
		var rid := RenderingServer.instance_create2(control.get_rid(), root.world_3d.scenario)
		RenderingServer.instance_set_visible(rid, false)
		controls.append(rid)
	var normal: Image
	for view in ["normal", "no-terrain-lod", "no-object-lod", "no-placed-lod", "unmasked-land"]:
		for rid: RID in controls:
			RenderingServer.instance_set_visible(rid, view == "unmasked-land")
		streamer.call("set_terrain_lod_visible", view != "no-terrain-lod")
		streamer.call("set_object_lod_visible", view != "no-object-lod")
		for rid: RID in placed_lod_rids:
			RenderingServer.instance_set_visible(rid, view != "no-placed-lod")
		for _frame in range(3):
			await process_frame
		await RenderingServer.frame_post_draw
		var image := root.get_texture().get_image()
		_check(image.save_png("res://.godot/whiterun-lod-" + label + "-" + view + ".png") == OK, "save capture")
		if view == "normal":
			normal = image
		if view == "unmasked-land":
			var changed := 0
			for y in range(image.get_height()):
				for x in range(image.get_width()):
					var a := normal.get_pixel(x, y)
					var b := image.get_pixel(x, y)
					changed += int(absf(a.r - b.r) + absf(a.g - b.g) + absf(a.b - b.b) > 0.12)
			print("WHITERUN_LAND_CONTROL ", label, " restored_triangles=", restored_triangles, " changed_pixels=", changed)
	for rid: RID in controls:
		RenderingServer.free_rid(rid)
	streamer.call("set_terrain_lod_visible", true)
	streamer.call("set_object_lod_visible", true)
	for rid: RID in placed_lod_rids:
		RenderingServer.instance_set_visible(rid, true)
	camera.global_transform = original


func _start() -> void:
	await super._start()
	if not loaded:
		quit(1)
		return
	var manifest: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(OS.get_environment("SOE_WHITERUN_LOD_REFERENCES")))
	reference_cells = manifest.references
	exterior_cells = manifest.cells
	full_lod_references = manifest.full_lod_references
	var exited := false
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			exited = player.call("activate_reference", door.file, door.object)
			break
	_check(exited, "authored inn exit to Plains District")
	player.call("advance", 0.0)
	await _settle_location()
	_check(player.call("detection_worldspace") == "WhiterunWorld", "Whiterun worldspace")
	_coverage("plains")
	await _captures("plains")
	_check(player.call("queue_player_input", {"action": "move_to", "file": 0, "object": 0x0E6C71, "offset": Vector3(-200, 0, 300), "match_rotation": true}), "pond walkway move")
	_check(player.call("step_player_input"), "pond walkway move applied")
	player.call("advance", 0.0)
	await _settle_location()
	_coverage("pond")
	await _captures("pond")
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "city gate found")
	if not gate.is_empty():
		_check(player.call("queue_player_input", {"action": "move_to", "file": gate.file, "object": gate.object, "offset": Vector3(0, 200, 0), "match_rotation": true}), "city-wall move")
		_check(player.call("step_player_input"), "city-wall move applied")
		player.call("advance", 0.0)
		await _settle_location()
		_coverage("walls")
		await _captures("walls")
		player.set("auto_step", true)
		_check(player.call("activate_reference", gate.file, gate.object), "cross city gate to parent world")
		player.call("advance", 0.0)
		await _settle_location()
		_check(player.call("detection_worldspace") == "Tamriel", "parent-world wall location")
		_coverage("outside-walls")
		await _captures("outside-walls")
		_check(player.call("queue_player_input", {"action": "move_to", "file": gate.far_file, "object": gate.far_object, "offset": Vector3(6 * 4096, 0, 1200), "match_rotation": true}), "move across the exterior grid")
		_check(player.call("step_player_input"), "exterior-grid move applied")
		player.call("advance", 0.0)
		_check_handoff()
		await _settle_location()
		_coverage("walking")
		var current: Vector3 = player.call("position")
		var focus := Vector2i(floori(current.x / 4096.0), floori(current.y / 4096.0))
		var handoff_reference := ""
		var target_cell := Vector2i.ZERO
		var nearest := 9
		for reference: String in streamer.call("distant_references"):
			if not full_lod_references.has(reference) or not reference_cells.has(reference):
				continue
			var coordinate: Array = reference_cells[reference]
			var candidate := Vector2i(int(coordinate[0]), int(coordinate[1]))
			var distance := maxi(absi(candidate.x - focus.x), absi(candidate.y - focus.y))
			if distance > 2 and distance < nearest:
				nearest = distance
				target_cell = candidate
				handoff_reference = reference
		_check(not handoff_reference.is_empty(), "nearby admitted Full LOD reference for handoff")
		if not handoff_reference.is_empty():
			var masked_before := masked_full_lod_members
			player.set("auto_step", true)
			var near_resident := false
			var pending_focus := focus
			var can_move := true
			for frame in range(1800):
				if not player.get("auto_step"):
					_check(false, "automatic exterior streaming stopped during handoff")
					break
				if can_move and maxi(absi(target_cell.x - focus.x), absi(target_cell.y - focus.y)) > 2:
					pending_focus = focus + Vector2i(signi(target_cell.x - focus.x), signi(target_cell.y - focus.y))
					var destination := Vector3((pending_focus.x + 0.5) * 4096.0, (pending_focus.y + 0.5) * 4096.0, current.z)
					var height: Variant = streamer.call("play_path_terrain_height", Vector3(destination.x * 0.0142875, 0, -destination.y * 0.0142875))
					_check(height != null, "walking destination has resident LAND")
					if height == null:
						break
					destination.z = float(height) + 160.0
					current = player.call("position")
					var queued: bool = player.call("queue_player_input", {"action": "move_to", "file": 0, "object": 0x14, "offset": destination - current, "match_rotation": true})
					var applied: bool = queued and player.call("step_player_input")
					_check(applied, "one-cell Full LOD handoff move applied")
					if not applied:
						break
					can_move = false
				await process_frame
				_check_handoff(true)
				var covered: String = streamer.call("terrain_lod_stats").get("covered_cells", "")
				var grid_ready := true
				var coordinates := covered.split(" ", false)
				for x in range(pending_focus.x - 2, pending_focus.x + 3):
					for y in range(pending_focus.y - 2, pending_focus.y + 3):
						grid_ready = grid_ready and coordinates.has(str(x) + ":" + str(y))
				if grid_ready:
					focus = pending_focus
					can_move = true
				for surface: Dictionary in streamer.call("play_path_surfaces"):
					if surface.get("pending_distant", false):
						continue
					if exterior_cells.has(surface.get("cell", "")) and surface.get("references", PackedStringArray()).has(handoff_reference):
						near_resident = true
				if frame >= 90 and near_resident and not player.call("is_location_preparing"):
					break
			player.set("auto_step", false)
			_check(near_resident, "Full LOD reference acquired near geometry")
			_check(masked_full_lod_members > masked_before, "observed immediate masking before distant replacement")
			print("WHITERUN_FULL_LOD_HANDOFF reference=", handoff_reference, " masked_members=", masked_full_lod_members - masked_before)
			_check(not streamer.call("distant_references").has(handoff_reference), "resident reference left Full LOD")
			_coverage("full-lod-handoff")
	print("WHITERUN_LOD_RESULT ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
