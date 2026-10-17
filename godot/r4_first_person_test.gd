extends "res://walkthrough.gd"

## The rendered rig's animated camera heading must stay on the player's view.
const MAX_HEADING_ERROR := PI / 180.0
var failures := 0
var results := {}


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("R4_FIRST_PERSON FAIL: ", message)


func _sample(label: String, actions: Array) -> void:
	player.set("auto_step", true)
	var worst := 0.0
	var moving := 0
	var clips := {}
	var path: Array[Dictionary] = []
	for action in actions:
		if label == "inn":
			player.call("set_facing", 0.7, -0.2)
		else:
			player.call("set_facing", 3.0 * PI / 2.0, 0.0)
		Input.action_press(action)
		if label == "exterior":
			await create_timer(0.5).timeout
		var start: Vector3 = player.call("position")
		var game_before: float = player.call("budget_runtime_status").seconds
		var deadline := Time.get_ticks_msec() + 1_000
		while Time.get_ticks_msec() < deadline:
			await process_frame
			await create_timer(0.0).timeout
			var observation: Dictionary = player_arms.call("play_path_observation")
			_check(observation.has("animated_camera_offset"), "animated camera bone is observed")
			if not observation.has("animated_camera_offset"):
				Input.action_release(action)
				quit(1)
				return
			var eye: Transform3D = observation.placement * observation.animated_camera_offset
			var camera := root.get_camera_3d()
			var rig_forward := -eye.basis.z
			var camera_forward := -camera.global_basis.z
			var error := absf(Vector2(rig_forward.x, rig_forward.z).angle_to(Vector2(camera_forward.x, camera_forward.z)))
			worst = maxf(worst, error)
			for clip in player_arms.call("active_clips"):
				clips[String(clip).get_slice("@", 0)] = true
		Input.action_release(action)
		var finish: Vector3 = player.call("position")
		path.append({"action": action, "start": start, "finish": finish, "grounded": player.call("is_on_ground"), "auto_step": player.get("auto_step"), "menu": player.call("is_menu_open"), "fade": player.call("door_fade_active"), "game_seconds": float(player.call("budget_runtime_status").seconds) - game_before})
		if Vector2(start.x, start.y).distance_to(Vector2(finish.x, finish.y)) > 10.0:
			moving += 1
	player.set("auto_step", false)
	results[label] = {"worst_heading_degrees": rad_to_deg(worst), "moving_directions": moving, "clips": clips.keys(), "path": path}
	print("R4_FIRST_PERSON ", label, " ", JSON.stringify(results[label]))
	_check(moving == actions.size(), label + " all requested directions moved")
	_check(worst < MAX_HEADING_ERROR, "animated rig heading stays within one degree of the camera")


func _start() -> void:
	Engine.max_fps = 60
	await super._start()
	_check(loaded, "walkthrough loaded")
	if not loaded:
		quit(1)
		return
	await _sample("inn", ["Forward", "Strafe Left", "Back", "Strafe Right"])
	var crossed := false
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			crossed = player.call("activate_reference", door.file, door.object)
			break
	_check(crossed, "Bannered Mare exit activated")
	player.set("auto_step", true)
	var deadline := Time.get_ticks_msec() + 120_000
	while (player.call("budget_runtime_status").world_editor_id != "WhiterunWorld" or player.call("is_location_preparing")) and Time.get_ticks_msec() < deadline:
		await process_frame
	_check(not player.call("is_location_preparing"), "exterior preparation finished")
	_check(player.call("budget_runtime_status").world_editor_id == "WhiterunWorld", "entered Whiterun")
	await _sample("exterior", ["Forward"])
	var path := OS.get_environment("SOE_R4_MOTION_REPORT")
	if not path.is_empty():
		var output := FileAccess.open(path, FileAccess.WRITE)
		_check(output != null, "report opened")
		if output != null:
			output.store_string(JSON.stringify(results, "\t"))
	print("R4_FIRST_PERSON ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
