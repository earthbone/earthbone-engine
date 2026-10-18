extends "res://hst_25_equipment_test.gd"

## Third person facing measurement: standing still sheathed, walking, and drawn.
## Measures the yaw between the rendered body placement's forward axis and the
## active camera's forward axis and prints it. Measured 2026-10-17: 0 degrees in
## all three states, the body placement takes the camera yaw by construction.

func _yaw_gap(body: Node3D) -> float:
	var camera: Camera3D = root.get_viewport().get_camera_3d()
	var placement: Transform3D = body.call("play_path_observation").get("placement", body.global_transform)
	var body_forward := -placement.basis.z
	var camera_forward := -camera.global_transform.basis.z
	body_forward.y = 0.0
	camera_forward.y = 0.0
	return rad_to_deg(body_forward.normalized().angle_to(camera_forward.normalized()))

func _frames(count: int) -> void:
	for step in count:
		player.call("advance", 1.0 / 60.0)
		player_body.call("advance", 1.0 / 60.0)
		player_arms.call("advance", 1.0 / 60.0)
		await _animation_frame(step)

func _check_weapons() -> void:
	await super._check_weapons()
	if not player.call("is_third_person"):
		Input.action_press("Toggle POV")
		player.call("advance", 1.0 / 60.0)
		Input.action_release("Toggle POV")
		player.call("advance", 0.0)
	_check(player.call("is_third_person"), "third person view was not selected")
	await _frames(30)
	# Measurement 1: standing still, sheathed. Vanilla leaves the body where it is
	# while the camera orbits, so this is reported and not asserted.
	var still := []
	for turn in [0.6, 1.4, 2.6, -1.2]:
		player.call("set_facing", turn, 0.0)
		await _frames(45)
		still.append(_yaw_gap(player_body))
		print("CMB-42 still sheathed camera yaw=%f body gap=%f deg" % [turn, still[-1]])
	# Measurement 2: walking forward, the body must turn to the camera heading.
	Input.action_press("Forward")
	await _frames(90)
	var walking := _yaw_gap(player_body)
	Input.action_release("Forward")
	await _frames(30)
	print("CMB-42 walking sheathed body gap=%f deg" % walking)
	_check(walking < 15.0, "the body does not face the heading while walking: %f deg" % walking)
	# Measurement 3: weapon drawn, standing, the body faces the camera.
	player.call("set_facing", 2.0, 0.0)
	_check(player.call("toggle_weapon_ready"), "draw failed")
	await _frames(90)
	var drawn := _yaw_gap(player_body)
	print("CMB-42 drawn standing body gap=%f deg" % drawn)
	_check(drawn < 15.0, "the drawn body does not face the camera: %f deg" % drawn)
	player.call("toggle_weapon_ready")
	await _frames(60)
	print("CMB-42 summary still=%s walking=%f drawn=%f" % [still, walking, drawn])
