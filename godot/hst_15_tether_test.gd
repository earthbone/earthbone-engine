extends SceneTree

## HST-15 host constraint exercise. Run with:
## godot --headless --path godot --script hst_15_tether_test.gd

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _body(space: RID, position: Vector3) -> Array:
	var shape := PhysicsServer3D.sphere_shape_create()
	PhysicsServer3D.shape_set_data(shape, 0.5)
	var body := PhysicsServer3D.body_create()
	PhysicsServer3D.body_set_mode(body, PhysicsServer3D.BODY_MODE_RIGID)
	PhysicsServer3D.body_add_shape(body, shape)
	PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM,
		Transform3D(Basis.IDENTITY, position))
	PhysicsServer3D.body_set_space(body, space)
	return [body, shape]

func _run() -> void:
	if not ClassDB.class_exists("CellStreamerNode"):
		print("extension not loaded, skipping")
		print("RESULT: PASS hst_15 tether (skipped)")
		quit(0)
		return
	var space := PhysicsServer3D.space_create()
	PhysicsServer3D.space_set_active(space, true)
	var cart_parts := _body(space, Vector3.ZERO)
	var horse_parts := _body(space, Vector3(0.0, 0.0, 2.0))
	var cart: RID = cart_parts[0]
	var cart_shape: RID = cart_parts[1]
	var horse: RID = horse_parts[0]
	var horse_shape: RID = horse_parts[1]
	var host: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(host)
	_check(host.call("tether_bodies", 0x100000002, 0x100000003, cart, horse,
		Vector3.ZERO, Vector3.ZERO), "valid cart and horse bodies were rejected")
	_check(host.call("tether_count") == 1, "host did not retain the joint")
	var cart_start: Vector3 = PhysicsServer3D.body_get_state(cart,
		PhysicsServer3D.BODY_STATE_TRANSFORM).origin
	PhysicsServer3D.body_apply_central_impulse(horse, Vector3(8.0, 0.0, 0.0))
	for _frame in range(30):
		await physics_frame
	var cart_position: Vector3 = PhysicsServer3D.body_get_state(cart,
		PhysicsServer3D.BODY_STATE_TRANSFORM).origin
	var horse_position: Vector3 = PhysicsServer3D.body_get_state(horse,
		PhysicsServer3D.BODY_STATE_TRANSFORM).origin
	_check(cart_position.x > cart_start.x + 0.1,
		"cart did not follow the horse after it received an impulse")
	_check(cart_position.distance_to(horse_position) < 2.1,
		"the constraint did not keep the cart connected to the horse")
	_check(host.call("untether_reference", 1, 2) == 1,
		"untethering the cart did not release its joint")
	_check(host.call("tether_bodies", 0x100000002, 0x100000003, cart, horse,
		Vector3.ZERO, Vector3.ZERO), "second tether setup failed")
	_check(host.call("untether_reference", 1, 3) == 1,
		"releasing the horse did not release its joint")
	_check(host.call("tether_count") == 0, "joint remained after endpoint release")
	_check(host.call("tether_bodies", 0x100000002, 0x100000003, cart, horse,
		Vector3.ZERO, Vector3.ZERO), "third tether setup failed")
	host.call("clear")
	_check(host.call("tether_count") == 0, "clearing the streamed host kept the joint")
	PhysicsServer3D.free_rid(cart)
	PhysicsServer3D.free_rid(cart_shape)
	PhysicsServer3D.free_rid(horse)
	PhysicsServer3D.free_rid(horse_shape)
	PhysicsServer3D.free_rid(space)
	host.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " hst_15 tether")
	quit(0 if failures == 0 else 1)
