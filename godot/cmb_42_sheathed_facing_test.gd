extends "res://hst_25_equipment_test.gd"

## Third person, weapon sheathed, standing still: the body must face the camera
## direction. Measures the yaw between the body's forward axis and the active
## camera's forward axis after turning the camera in steps, and prints it.

func _yaw_gap(body: Node3D) -> float:
	var camera := get_viewport().get_camera_3d()
	var body_forward := -body.global_transform.basis.z
	var camera_forward := -camera.global_transform.basis.z
	body_forward.y = 0.0
	camera_forward.y = 0.0
	return rad_to_deg(body_forward.normalized().angle_to(camera_forward.normalized()))

func _check_weapons() -> void:
	await super._check_weapons()
	_check(not player.call("weapon_ready") if player.has_method("weapon_ready") else true, "weapon must start sheathed")
	player.call("set_third_person", true) if player.has_method("set_third_person") else null
	await _steps(30)
	for turn in [0.6, 1.4, 2.6]:
		player.call("look_to", turn, 0.0) if player.has_method("look_to") else null
		await _steps(45)
		var gap := _yaw_gap(player_body)
		print("CMB-42 camera turn=%f body gap=%f deg" % [turn, gap])
		_check(gap < 15.0, "sheathed idle body does not face the camera: %f deg at turn %f" % [gap, turn])
