extends SceneTree

## AI-70 verifies that player sight rays use the live host camera and physics.
var failures := 0


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL ai_70: ", message)


func _make_wall(space: RID, position: Vector3) -> Dictionary:
	var shape := PhysicsServer3D.box_shape_create()
	PhysicsServer3D.shape_set_data(shape, Vector3(0.5, 0.5, 0.15))
	var body := PhysicsServer3D.body_create()
	PhysicsServer3D.body_set_mode(body, PhysicsServer3D.BODY_MODE_STATIC)
	PhysicsServer3D.body_add_shape(body, shape)
	PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM,
		Transform3D(Basis.IDENTITY, position))
	PhysicsServer3D.body_set_space(body, space)
	return {"body": body, "shape": shape}


func _free_wall(wall: Dictionary) -> void:
	PhysicsServer3D.free_rid(wall.body)
	PhysicsServer3D.free_rid(wall.shape)


func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL ai_70: installed data and the host extension are required")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "installed data load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared records")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"Bannered Mare collision")
	_check(player.call("start_gameplay"), "gameplay startup")
	await physics_frame
	var camera: Camera3D = player.get_viewport().get_camera_3d()
	_check(camera != null, "live player camera")
	if camera == null:
		player.free()
		streamer.free()
		quit(1)
		return
	# Move the live camera above the loaded scene so the controlled ray has no
	# unrelated geometry, then put one known blocker directly on that ray.
	camera.global_transform = Transform3D(Basis.IDENTITY, Vector3(0.0, 10000.0, 0.0))
	var target := Vector3(0.0, 10000.0, -10.0)
	var space: RID = player.get_world_3d().space
	await physics_frame
	_check(player.call("camera_line_of_sight_to_point", target),
		"unobstructed camera ray")
	var wall := _make_wall(space, Vector3(0.0, 10000.0, -5.0))
	await physics_frame
	_check(not player.call("camera_line_of_sight_to_point", target),
		"wall blocks camera ray")
	_free_wall(wall)
	await physics_frame
	_check(player.call("camera_line_of_sight_to_point", target),
		"camera ray clears after blocker removal")
	print("ai_70_camera_los_test: ", "PASS" if failures == 0 else "FAIL")
	player.free()
	streamer.free()
	quit(0 if failures == 0 else 1)
