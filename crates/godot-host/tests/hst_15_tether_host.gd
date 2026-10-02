extends SceneTree

## The host pin joint keeps two bodies together and releases on either endpoint.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _body(space: RID, origin: Vector3, shapes: Array[RID]) -> RID:
	var shape := PhysicsServer3D.sphere_shape_create()
	shapes.append(shape)
	PhysicsServer3D.shape_set_data(shape, 0.25)
	var body := PhysicsServer3D.body_create()
	PhysicsServer3D.body_set_mode(body, PhysicsServer3D.BODY_MODE_RIGID)
	PhysicsServer3D.body_set_space(body, space)
	PhysicsServer3D.body_add_shape(body, shape)
	var pose := Transform3D.IDENTITY
	pose.origin = origin
	PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM, pose)
	return body

func _run() -> void:
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	var space := get_root().get_world_3d().space
	var shapes: Array[RID] = []
	var cart := _body(space, Vector3(0.0, 1.0, 0.0), shapes)
	var horse := _body(space, Vector3(0.0, 1.3, -3.5), shapes)
	var cart_pivot := Vector3(0.0, -0.2, -3.5)
	var horse_pivot := Vector3(0.0, -0.5, 0.0)
	_check(streamer.call("tether_bodies", 0x900, 0x901, cart, horse,
		cart_pivot, horse_pivot), "host rejected resident cart and horse bodies")
	_check(streamer.call("tether_count") == 1, "host did not retain the pin joint")
	PhysicsServer3D.body_apply_central_impulse(horse, Vector3(0.1, 0.0, 0.0))
	for _frame in range(30):
		await physics_frame
	var cart_pose: Transform3D = PhysicsServer3D.body_get_state(cart, PhysicsServer3D.BODY_STATE_TRANSFORM)
	var horse_pose: Transform3D = PhysicsServer3D.body_get_state(horse, PhysicsServer3D.BODY_STATE_TRANSFORM)
	var before_distance := (cart_pose.origin + cart_pivot).distance_to(horse_pose.origin + horse_pivot)
	_check(horse_pose.origin.x > 0.005, "horse body did not move under the applied impulse")
	_check(before_distance < 0.1, "pin joint did not keep the cart attached to the moving horse")
	_check(streamer.call("untether_reference", 0, 0x901) == 1,
		"untethering the horse did not release the host constraint")
	_check(streamer.call("tether_bodies", 0x900, 0x901, cart, horse,
		cart_pivot, horse_pivot), "host could not recreate the released tether")
	_check(streamer.call("untether_reference", 0, 0x900) == 1,
		"releasing the cart endpoint did not remove the host constraint")
	_check(streamer.call("tether_bodies", 0x900, 0x901, cart, horse,
		cart_pivot, horse_pivot), "host could not recreate the tether before cell clear")
	streamer.call("clear")
	_check(streamer.call("tether_count") == 0, "clearing the host cell retained its tether")
	PhysicsServer3D.free_rid(cart)
	PhysicsServer3D.free_rid(horse)
	for shape in shapes:
		PhysicsServer3D.free_rid(shape)
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " hst_15 host tether")
	quit(0 if failures == 0 else 1)
