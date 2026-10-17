# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"
## Load-door regression through the ordinary free walk activation path.

const UNITS := 69.99125

var failures := 0
var found_requested := false
var last_arrival := Vector3.ZERO

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _process(_delta: float) -> bool:
	return false

func _tick() -> void:
	player.call("advance", 1.0 / 60.0)
	await physics_frame

func _settle() -> bool:
	var started := Time.get_ticks_msec()
	while player.call("is_load_door_pending") or player.call("is_location_preparing"):
		if Time.get_ticks_msec() - started > 90_000:
			return false
		await _tick()
	return player.call("gameplay_location_ready")

func _cross(door: Dictionary) -> bool:
	var before: String = player.call("gameplay_cell")
	var authored: Dictionary = door if door.has("return_position") else {}
	for candidate: Dictionary in player.call("gameplay_doors"):
		if candidate.file == door.file and candidate.object == door.object:
			authored = candidate
	assert(not authored.is_empty(), "each side must resolve its authored door")
	var pose: Dictionary = player.call("gameplay_reference_pose", door.file, door.object)
	assert(player.call("queue_player_input", {"action": "move_to", "file": door.file,
		"object": door.object, "offset": authored.return_position - pose.position, "match_rotation": false}))
	assert(player.call("step_player_input"))
	for frame in range(30):
		await _tick()
	assert(player.call("is_on_ground"), "authored approach must have supporting collision")
	var state_before: Dictionary = streamer.call("door_state", door.file, door.object)
	var vertices: PackedVector3Array = state_before.world_vertices
	assert(not vertices.is_empty(), "each exit must have submitted geometry")
	var bounds := AABB(vertices[0], Vector3.ZERO)
	for vertex in vertices:
		bounds = bounds.expand(vertex)
	var eye_m: Vector3 = root.get_camera_3d().global_position
	var eye := Vector3(eye_m.x, -eye_m.z, eye_m.y) * UNITS
	var center := bounds.get_center()
	center.y = clampf(eye_m.y, bounds.position.y + 0.05, bounds.end.y - 0.05)
	var right := (center - eye_m).cross(Vector3.UP).normalized()
	var width := bounds.size.dot(right.abs())
	var visible_target := false
	for fraction in [0.0, -0.15, 0.15, -0.3, 0.3]:
		var point: Vector3 = center + right * width * fraction
		var end := eye_m + (point - eye_m).normalized() * 192.0 / UNITS
		var visual: Dictionary = streamer.call("rendered_door_pick", eye_m, end)
		if visual.get("file", -1) == door.file and visual.get("object", -1) == door.object:
			center = point
			visible_target = true
			break
	assert(visible_target, "the reachable visible door must have an aim point")
	var target := Vector3(center.x, -center.z, center.y) * UNITS
	var direction := target - eye
	player.call("set_facing", atan2(direction.x, direction.y), atan2(direction.z, Vector2(direction.x, direction.y).length()))
	await _tick()
	await RenderingServer.frame_post_draw
	var output := OS.get_environment("SOE_R4_OUTPUT")
	if not output.is_empty():
		DirAccess.make_dir_recursive_absolute(output)
		assert(root.get_texture().get_image().save_png(output.path_join("door-%06X.png" % door.object)) == OK)
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
	var activated := picked == "%d:%06X" % [door.file, door.object]
	var settled := await _settle()
	var after: String = player.call("gameplay_cell")
	var landing: Vector3 = player.call("position")
	last_arrival = landing
	if activated and settled and before != after:
		for frame in range(120):
			await _tick()
	var grounded: bool = player.call("is_on_ground")
	var feet: Vector3 = player.call("position")
	var navigation: Dictionary = player.call("play_path_navigation_details", feet)
	var supported := grounded and absf(feet.z - landing.z) < 256.0
	var navigable: bool = not navigation.get("points", PackedVector3Array()).is_empty()
	var passed: bool = activated and settled and before != after and supported and navigable
	print("R4_DOOR ", JSON.stringify({"door": door, "from": before, "to": after,
		"activated": activated, "picked": picked, "settled": settled, "pass": passed,
		"animation_before": state_before,
		"animation_after": streamer.call("door_state", door.file, door.object),
		"position": feet, "landing": landing, "grounded": grounded, "navigation": navigation}))
	return passed

func _start() -> void:
	await super._start()
	if not loaded:
		return
	print("R4_DOOR requested_pose=", player.call("gameplay_reference_pose", 0, 0x0056DF3))
	for cell in ["WhiterunBanneredMare", "WhiterunDragonsreach", "WhiterunHeimskrsHouse"]:
		if player.call("gameplay_cell") != cell:
			print("R4_DOOR coc=", player.call("console_execute", "coc " + cell))
			assert(await _settle(), "requested survey cell must finish loading")
		for reference: Dictionary in player.call("play_path_references"):
			found_requested = found_requested or reference.object == 0x0056DF3
		if cell == "WhiterunHeimskrsHouse":
			print("R4_DOOR authored_key=", player.call("console_execute", "player.additem 00093B0A 1"))
		var doors: Array = player.call("gameplay_doors")
		print("R4_DOOR survey cell=", cell, " doors=", JSON.stringify(doors))
		for door: Dictionary in doors:
			found_requested = found_requested or door.object == 0x0056DF3 or door.far_object == 0x0056DF3
			var crossed := await _cross(door)
			if not crossed:
				failures += 1
				continue
			var returning := {"file": door.far_file, "object": door.far_object, "return_position": last_arrival}
			if not await _cross(returning):
				failures += 1
				break
	if not found_requested:
		failures += 1
		printerr("R4_DOOR requested 0056DF3 missing from survey")
	print("R4_DOOR failures=", failures)
	quit(0 if failures == 0 else 1)
