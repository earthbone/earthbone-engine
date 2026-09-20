extends SceneTree

## CMB-30: query bodies follow gameplay startup, door replacement and teardown.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _ray(space: RID, from: Vector3, to: Vector3, excluded: Array[RID] = []) -> Dictionary:
	var query := PhysicsRayQueryParameters3D.create(from, to)
	query.hit_from_inside = true
	query.exclude = excluded
	return PhysicsServer3D.space_get_direct_state(space).intersect_ray(query)

func _probe(player: Node3D) -> Dictionary:
	await physics_frame
	await physics_frame
	var bodies: Array[RID] = player.call("contact_collision_bodies")
	_check(not bodies.is_empty(), "loaded location has no query bodies")
	_check(player.call("contact_collision_shape_count") >= bodies.size(), "missing query shapes")
	var feet: Vector3 = player.call("position")
	var from := Vector3(feet.x, feet.z, -feet.y) * 0.0142875 + Vector3.UP
	var to := from + Vector3.DOWN * 50.0
	var space := player.get_world_3d().space
	var hit := _ray(space, from, to)
	_check(not hit.is_empty(), "location has no first obstruction below the player")
	if hit.is_empty():
		return {"from": from, "to": to, "bodies": bodies}
	_check(hit.rid in bodies, "first obstruction is not owned by the current location")
	var probe_shape := PhysicsServer3D.sphere_shape_create()
	PhysicsServer3D.shape_set_data(probe_shape, 0.1)
	var probe_body := PhysicsServer3D.body_create()
	PhysicsServer3D.body_set_mode(probe_body, PhysicsServer3D.BODY_MODE_STATIC)
	PhysicsServer3D.body_add_shape(probe_body, probe_shape)
	var centre: Vector3 = hit.position + Vector3.DOWN
	PhysicsServer3D.body_set_state(probe_body, PhysicsServer3D.BODY_STATE_TRANSFORM,
		Transform3D(Basis.IDENTITY, centre))
	PhysicsServer3D.body_set_space(probe_body, space)
	await physics_frame
	await physics_frame
	var obstructed := _ray(space, from, centre + Vector3.DOWN)
	var unobstructed := _ray(space, from, centre + Vector3.DOWN, bodies)
	_check(not obstructed.is_empty() and obstructed.rid in bodies,
		"query reached a body behind the first world obstruction")
	_check(not unobstructed.is_empty() and unobstructed.rid == probe_body,
		"the behind-wall probe was not on the tested ray")
	PhysicsServer3D.free_rid(probe_body)
	PhysicsServer3D.free_rid(probe_shape)
	print("CMB30 location=", player.call("gameplay_cell"), " bodies=", bodies.size(),
		" shapes=", player.call("contact_collision_shape_count"))
	return {"from": from, "to": to, "bodies": bodies}

func _replaced(space: RID, old: Dictionary, current: Array[RID]) -> void:
	for body: RID in old.bodies:
		_check(body not in current, "replaced location retained an old body RID")
	_check(_ray(space, old.from, old.to, current).is_empty(),
		"replaced location still obstructs queries after excluding the new bodies")

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: CMB30 requires installed data and the host extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	print("CMB30 loading installed data")
	_check(streamer.call("load_data", data), "data load failed")
	print("CMB30 preparing initial collision")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"initial collision failed")
	var initial := await _probe(player)
	var initial_count: int = initial.bodies.size()
	_check(player.call("start_gameplay"), "gameplay startup failed")
	_check(player.call("contact_collision_bodies") == initial.bodies,
		"gameplay startup duplicated the already installed collision")
	var start: String = player.call("gameplay_cell")
	var doors: Array = player.call("gameplay_doors")
	var chosen: Dictionary = {}
	for door: Dictionary in doors:
		if not door.locked:
			chosen = door
			break
	_check(not chosen.is_empty(), "no unlocked load door")
	var last := initial
	var space := player.get_world_3d().space
	if not chosen.is_empty():
		_check(player.call("activate_reference", chosen.file, chosen.object), "outbound door failed")
		_check(player.call("gameplay_cell") != start, "outbound door kept the old cell")
		last = await _probe(player)
		_replaced(space, initial, last.bodies)
		_check(player.call("activate_reference", chosen.far_file, chosen.far_object), "return door failed")
		_check(player.call("gameplay_cell") == start, "return door kept the exterior")
		var returned := await _probe(player)
		_check(returned.bodies.size() == initial_count, "round trip changed the interior body count")
		_replaced(space, last, returned.bodies)
		last = returned
	player.free()
	await physics_frame
	await physics_frame
	_check(_ray(space, last.from, last.to).is_empty(), "player exit left static query bodies")
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " CMB30 static contact queries")
	quit(0 if failures == 0 else 1)
