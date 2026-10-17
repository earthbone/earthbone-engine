# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://r4_giant_behaviour_test.gd"

## Inspect the exact uploaded skin in the free walk, before any skin correction.
const MAX_EDGE_STRETCH := 8.0
const EDGE_STRETCH_P99 := 2.5
const MIN_EDGE_UNITS := 0.25
var bind_arrays := {}

func _measure() -> void:
	startup_position = Vector3(-125, -800, 120)
	await super._start()
	if not loaded:
		return
	print("R4_SKIN combat disabled=", player.call("debug_view_command", "tcai"))
	var camp := ""
	for cell: Dictionary in streamer.call("editor_cell_tree"):
		if not cell.interior and str(cell.name).to_lower().contains("bleakwindbasin"):
			camp = cell.name
			break
	if camp.is_empty():
		quit(1)
		return
	print("R4_SKIN travel=", player.call("console_execute", "coc " + camp))
	player.call("advance", 0.0)
	while player.call("is_location_preparing"):
		await process_frame
	for _frame in 180:
		player.call("advance", STEP)
		await process_frame
	var references: Array = player.call("play_path_references")
	for visual: Dictionary in player.call("npc_visuals"):
		if visual.node_id == 0:
			continue
		var actor: Node = instance_from_id(visual.node_id)
		if not str(actor.call("skeleton_path")).to_lower().contains("/giant/"):
			continue
		for reference: Dictionary in references:
			if reference.file == visual.file and reference.object == visual.object:
				giant = reference
				giant_node = actor
				break
		if not giant.is_empty():
			break
	if giant.is_empty():
		quit(1)
		return
	giant_id = str(giant.file) + ":%06X" % int(giant.object)
	await _pose_phase("idle", 120)
	player.call("debug_view_command", "prid %08X" % int(giant.object))
	player.call("debug_view_command", "setav aggression 3")
	_place(900.0)
	player.call("debug_view_command", "tcai")
	await _pose_phase("walking", 300)
	print("R4_SKIN_RESULT failures=", failures)
	quit(0 if failures == 0 else 1)

func _pose_phase(label: String, count: int) -> void:
	var walking := false
	var initial: Transform3D = giant_node.call("play_path_observation").placement
	var moved := 0.0
	for frame in count:
		if label == "walking" and frame % 30 == 0:
			_key(KEY_D, frame % 60 == 0)
			_key(KEY_A, frame % 60 != 0)
		player.call("advance", STEP)
		await process_frame
		var observation: Dictionary = giant_node.call("play_path_observation")
		var placement: Transform3D = observation.placement
		moved = maxf(moved, initial.origin.distance_to(placement.origin) / 0.0142875)
		var clips := str(giant_node.call("active_clips")).to_lower()
		walking = walking or clips.contains("walk") or clips.contains("run")
		if frame % 60 == 59:
			var metrics := _skin_metrics(observation)
			print("R4_SKIN_POSE ", JSON.stringify({"phase": label, "frame": frame, "clips": clips, "metrics": metrics}))
			if metrics.is_empty() or float(metrics.get("max", INF)) > MAX_EDGE_STRETCH or float(metrics.get("p99", INF)) > EDGE_STRETCH_P99 or int(metrics.get("invalid", 1)) != 0:
				failures += 1
	_key(KEY_D, false)
	_key(KEY_A, false)
	if label == "walking" and (not walking or moved < 64.0):
		failures += 1
	print("R4_SKIN_PHASE ", JSON.stringify({"phase": label, "walking": walking, "moved_units": moved}))

func _skin_metrics(observation: Dictionary) -> Dictionary:
	if observation.shapes.is_empty() or observation.poses.is_empty():
		return {}
	var mesh: ArrayMesh = observation.shapes[0].mesh
	var identity := mesh.get_instance_id()
	if not bind_arrays.has(identity):
		bind_arrays[identity] = mesh.surface_get_arrays(0)
	var arrays: Array = bind_arrays[identity]
	var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	var bones: PackedInt32Array = arrays[Mesh.ARRAY_BONES]
	var weights: PackedFloat32Array = arrays[Mesh.ARRAY_WEIGHTS]
	var triangles: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
	var poses: Array = observation.poses
	if bones.size() != vertices.size() * 4 or weights.size() != bones.size():
		return {}
	var deformed := PackedVector3Array()
	deformed.resize(vertices.size())
	var invalid := 0
	for vertex in vertices.size():
		var point := Vector3.ZERO
		var total := 0.0
		for slot in 4:
			var index := vertex * 4 + slot
			var bone := bones[index]
			var weight := weights[index]
			if weight <= 0.0:
				continue
			if bone < 0 or bone >= poses.size():
				invalid += 1
				continue
			point += (poses[bone] * vertices[vertex]) * weight
			total += weight
		if total < 0.99 or total > 1.01 or not point.is_finite():
			invalid += 1
		deformed[vertex] = point
	var ratios: Array[float] = []
	for triangle in range(0, triangles.size(), 3):
		for pair in [[triangles[triangle], triangles[triangle + 1]], [triangles[triangle + 1], triangles[triangle + 2]], [triangles[triangle + 2], triangles[triangle]]]:
			var a: int = pair[0]
			var b: int = pair[1]
			var rest := vertices[a].distance_to(vertices[b])
			if rest >= MIN_EDGE_UNITS * 0.0142875:
				ratios.append(deformed[a].distance_to(deformed[b]) / rest)
	if ratios.is_empty():
		return {}
	ratios.sort()
	return {"max": ratios[-1], "p99": ratios[int(ceil(ratios.size() * 0.99)) - 1], "edges": ratios.size(), "invalid": invalid, "vertices": vertices.size()}
