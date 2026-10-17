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
		if line.begins_with("CELL: (000165AD) "):
			cell_name = line.trim_prefix("CELL: (000165AD) ").get_slice(" ", 0)
	assert(not cell_name.is_empty(), "reported reference's authored cell must exist")
	print("R4_PICK target_cell=", cell_name, " coc=", player.call("console_execute", "coc " + cell_name))
	var preparing_started := Time.get_ticks_msec()
	while player.call("is_location_preparing"):
		assert(Time.get_ticks_msec() - preparing_started < 90_000)
		await _tick()
	var clicked: Dictionary = {}
	for reference: Dictionary in player.call("play_path_references"):
		if reference.file == 0 and reference.object == 0x056DF3:
			clicked = reference
			print("R4_PICK reported_reference=", JSON.stringify(reference))
	assert(not clicked.is_empty(), "reported doorway must be resident")
	var doors: Array = player.call("gameplay_doors")
	assert(not doors.is_empty(), "authored interior exit must exist")
	doors.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		var a_pose: Dictionary = player.call("gameplay_reference_pose", a.file, a.object)
		var b_pose: Dictionary = player.call("gameplay_reference_pose", b.file, b.object)
		return clicked.position.distance_squared_to(a_pose.position) < clicked.position.distance_squared_to(b_pose.position))
	var door: Dictionary = doors[0]
	print("R4_PICK authored_door=", JSON.stringify(door))

	var pose: Dictionary = player.call("gameplay_reference_pose", door.file, door.object)
	var offset: Vector3 = door.return_position - pose.position
	assert(player.call("queue_player_input", {"action": "move_to", "file": door.file,
		"object": door.object, "offset": offset, "match_rotation": false}))
	assert(player.call("step_player_input"))
	for frame in range(30):
		await _tick()
	assert(player.call("is_on_ground"), "authored return marker must have supporting collision")
	var state: Dictionary = streamer.call("door_state", door.file, door.object)
	var vertices: PackedVector3Array = state.world_vertices
	assert(not vertices.is_empty(), "exit must have resident geometry")
	var bounds := AABB(vertices[0], Vector3.ZERO)
	for vertex in vertices:
		bounds = bounds.expand(vertex)
	var camera := root.get_camera_3d()
	var eye_m := camera.global_position
	var eye := Vector3(eye_m.x, -eye_m.z, eye_m.y) * UNITS
	var center := bounds.get_center()
	var target := Vector3(center.x, -center.z, center.y) * UNITS
	var direction := target - eye
	player.call("set_facing", atan2(direction.x, direction.y), atan2(direction.z, Vector2(direction.x, direction.y).length()))
	await _tick()
	print("R4_PICK before=", player.call("gameplay_cell"), " grounded=", player.call("is_on_ground"),
		" feet=", player.call("position"), " target=", target, " ray=", player.call("carry_status"),
		" prompt=", player.call("activation_prompt"), " collision=",
		player.call("reference_collision_ray", door.file, door.object, eye_m, center))
	await RenderingServer.frame_post_draw
	var output := OS.get_environment("SOE_R4_OUTPUT")
	if not output.is_empty():
		DirAccess.make_dir_recursive_absolute(output)
		assert(root.get_texture().get_image().save_png(output.path_join("door.png")) == OK)
	var before: String = player.call("gameplay_cell")
	var visual: Dictionary = streamer.call("rendered_door_pick", eye_m, center)
	assert(visual.get("object", 0) == door.object, "drawn exit must win over its convex frame")
	var wall := eye_m + camera.global_basis.x * 2.0
	var blocked: Dictionary = streamer.call("rendered_door_pick", wall, center)
	assert(not blocked.is_empty() and blocked.get("object", 0) != door.object, "opaque side wall must occlude the exit")
	var key := InputEventKey.new()
	key.keycode = KEY_E
	key.pressed = true
	Input.parse_input_event(key)
	Input.flush_buffered_events()
	player.call("advance", 1.0 / 60.0)
	key = InputEventKey.new()
	key.keycode = KEY_E
	key.pressed = false
	Input.parse_input_event(key)
	Input.flush_buffered_events()
	player.call("advance", 1.0 / 60.0)
	var picked: String = player.call("last_activate_reference")
	var started := Time.get_ticks_msec()
	while player.call("is_load_door_pending") or player.call("is_location_preparing"):
		assert(Time.get_ticks_msec() - started < 90_000, "E activation must finish")
		await _tick()
	var after: String = player.call("gameplay_cell")
	assert(picked == "%d:%06X" % [door.file, door.object], "E must select the drawn door")
	assert(before == after, "authored novice lock must reject entry without its key")
	print("R4_PICK locked_without_key=true")
	player.call("close_menu")
	await _tick()
	print("R4_PICK authored_key=", player.call("console_execute", "player.additem 00093B0A 1"))
	key = InputEventKey.new()
	key.keycode = KEY_E
	key.pressed = true
	Input.parse_input_event(key)
	Input.flush_buffered_events()
	player.call("advance", 1.0 / 60.0)
	key = InputEventKey.new()
	key.keycode = KEY_E
	key.pressed = false
	Input.parse_input_event(key)
	Input.flush_buffered_events()
	player.call("advance", 1.0 / 60.0)
	picked = player.call("last_activate_reference")
	started = Time.get_ticks_msec()
	while player.call("is_load_door_pending") or player.call("is_location_preparing"):
		assert(Time.get_ticks_msec() - started < 90_000, "E activation must finish")
		await _tick()
	after = player.call("gameplay_cell")
	var passed := picked == "%d:%06X" % [door.file, door.object] and before != after
	print("R4_PICK picked=", picked, " expected=%d:%06X" % [door.file, door.object],
		" before=", before, " after=", after, " pass=", passed)
	quit(0 if passed else 1)
