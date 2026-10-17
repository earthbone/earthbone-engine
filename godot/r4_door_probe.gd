# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"
## E-key activation beside the reported static doorway in its authored cell.

const UNITS := 69.99125

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _process(_delta: float) -> bool:
	return false

func _tick() -> void:
	player.call("advance", 1.0 / 60.0)
	await physics_frame

func _start() -> void:
	await super._start()
	if not loaded:
		return
	var cell_name := ""
	var listing: String = player.call("console_execute", 'help "" 4 CELL')
	for line in listing.split("\n"):
		if line.begins_with("CELL: (000165A3) "):
			cell_name = line.trim_prefix("CELL: (000165A3) ").get_slice(" ", 0)
	assert(not cell_name.is_empty(), "reported reference's authored cell must exist")
	print("R4_PICK target_cell=", cell_name, " coc=", player.call("console_execute", "coc " + cell_name))
	var preparing_started := Time.get_ticks_msec()
	while player.call("is_location_preparing"):
		assert(Time.get_ticks_msec() - preparing_started < 90_000)
		await _tick()
	var doors: Array = player.call("gameplay_doors")
	var references: Array = player.call("play_path_references")
	for door: Dictionary in doors:
		if door.object not in [0x016707, 0x046B84, 0x08108A]:
			continue
		var pose: Dictionary = player.call("gameplay_reference_pose", door.file, door.object)
		assert(player.call("queue_player_input", {"action": "move_to", "file": door.file,
			"object": door.object, "offset": door.return_position - pose.position, "match_rotation": false}))
		assert(player.call("step_player_input"))
		for frame in range(30):
			await _tick()
		var state: Dictionary = streamer.call("door_state", door.file, door.object)
		var vertices: PackedVector3Array = state.world_vertices
		var bounds := AABB(vertices[0], Vector3.ZERO)
		for vertex in vertices:
			bounds = bounds.expand(vertex)
		var eye_m := root.get_camera_3d().global_position
		var center := bounds.get_center()
		center.y = clampf(eye_m.y, bounds.position.y + 0.05, bounds.end.y - 0.05)
		var right := (center - eye_m).cross(Vector3.UP).normalized()
		var width := bounds.size.dot(right.abs())
		for fraction in [0.0, -0.15, 0.15, -0.3, 0.3]:
			var point: Vector3 = center + right * width * fraction
			var direction := point - eye_m
			var sky := Vector3(direction.x, -direction.z, direction.y)
			player.call("set_facing", atan2(sky.x, sky.y), atan2(sky.z, Vector2(sky.x, sky.y).length()))
			await _tick()
			var visual: Dictionary = streamer.call("rendered_door_pick", eye_m, eye_m + direction.normalized() * 192.0 / UNITS)
			var status: Dictionary = player.call("carry_status")
			var selected: Dictionary = {}
			for reference: Dictionary in references:
				if reference.file == status.get("crosshair_file", -1) and reference.object == status.get("crosshair_object", -1):
					selected = reference
			print("R4_PROBE door=%06X fraction=%s distance=%s visual=%s status=%s selected=%s" % [door.object, fraction, direction.length() * UNITS, visual, status, selected])
	quit()
