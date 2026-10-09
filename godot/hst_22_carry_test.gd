extends SceneTree

## HST-22 installed tankard acceptance through PlayerNode's mapped Activate.
## The Rust harness selects the tankard from its winning base editor ID.

const UNITS_PER_METRE := 69.99125
const STEP := 1.0 / 60.0
var streamer: Node3D
var player: Node3D
var reference_file: int
var reference_object: int
var body: RID
var failures := 0

func _initialize() -> void:
	call_deferred("hst_22_carry_two_metres_collide_drop_and_tap")

func _check(ok: bool, message: String) -> bool:
	if not ok:
		printerr("HST-22 FAIL: ", message)
		failures += 1
		quit(1)
	return ok

func _tick(count := 1) -> void:
	for i in count:
		await physics_frame
		player.call("advance", STEP)

func _pose() -> Dictionary:
	for row: Dictionary in streamer.call("movable_reference_poses"):
		if row.file == reference_file and row.object == reference_object:
			return row
	return {}

func _matches(status: Dictionary, prefix: String) -> bool:
	return status.get(prefix + "_file", -1) == reference_file and status.get(prefix + "_object", -1) == reference_object

func _key(pressed: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = KEY_E
	event.pressed = pressed
	Input.parse_input_event(event)
	Input.flush_buffered_events()

func _skyrim(v: Vector3) -> Vector3:
	return Vector3(v.x, -v.z, v.y) * UNITS_PER_METRE

func _godot(v: Vector3) -> Vector3:
	return Vector3(v.x, v.z, -v.y) / UNITS_PER_METRE

func _body_center() -> Vector3:
	var transform: Transform3D = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM)
	var total := Vector3.ZERO
	var count := 0
	for i in PhysicsServer3D.body_get_shape_count(body):
		var shape: RID = PhysicsServer3D.body_get_shape(body, i)
		var data = PhysicsServer3D.shape_get_data(shape)
		var placement := transform * PhysicsServer3D.body_get_shape_transform(body, i)
		if data is PackedVector3Array:
			for point: Vector3 in data:
				total += placement * point
				count += 1
	return _skyrim(total / float(count)) if count > 0 else _skyrim(transform.origin)

func _approach() -> void:
	_check(player.call("queue_player_input", {"action": "move_to", "file": reference_file, "object": reference_object, "offset": Vector3(0, -85, 5), "match_rotation": false}), "approach queued")
	_check(player.call("step_player_input"), "approach applied")
	await _tick(60)

func _aim(point: Vector3) -> void:
	var eye := _skyrim(get_root().get_camera_3d().global_position)
	var direction := point - eye
	var yaw := atan2(direction.x, direction.y)
	var pitch := atan2(direction.z, Vector2(direction.x, direction.y).length())
	_check(player.call("queue_player_input", {"action": "turn", "rotation": Vector3(-pitch, 0, yaw)}), "turn queued")
	_check(player.call("step_player_input"), "turn applied")
	await _tick()

func hst_22_carry_two_metres_collide_drop_and_tap() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("HST-22 SKIP: SKYRIM_DATA unset")
		quit(0)
		return
	reference_file = int(OS.get_environment("SOE_HST_22_FILE"))
	reference_object = int(OS.get_environment("SOE_HST_22_OBJECT"))
	var root := Node3D.new()
	get_root().add_child(root)
	streamer = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	if not _check(streamer.call("load_data", data), "data load"):
		return
	if not _check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0 and streamer.call("stream_cell_collision", "WhiterunBanneredMare") > 0, "cell stream"):
		return
	if not _check(not _pose().is_empty(), "authored tankard body"):
		return
	var initial: Vector3 = _pose().position
	await physics_frame
	var spawn := initial + Vector3(-85, 0, -10)
	var floor_query := PhysicsRayQueryParameters3D.create(_godot(spawn), _godot(spawn - Vector3(0, 0, 500)))
	var floor_hit := root.get_world_3d().direct_space_state.intersect_ray(floor_query)
	if not _check(not floor_hit.is_empty(), "floor below tankard level"):
		return
	spawn.z = _skyrim(floor_hit.position).z + 5.0
	print("HST-22 spawn=", spawn)
	player = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	if not _check(player.call("use_streamer_data", streamer), "shared source"):
		return
	if not _check(player.call("enter_cell", "WhiterunBanneredMare", spawn.x, spawn.y, spawn.z) > 0 and player.call("start_gameplay"), "gameplay start"):
		return
	await _tick(60)
	body = _pose().body
	_check(PhysicsServer3D.body_get_mode(body) == PhysicsServer3D.BODY_MODE_RIGID, "tankard is dynamic Jolt body")
	initial = _pose().position
	# The rigid body origin can be below the model's visible cup.
	await _aim(_body_center())
	print("HST-22 aim: player=", player.call("position"), " body=", _pose().position, " center=", _body_center(), " facing=", player.call("facing"))
	get_root().get_texture().get_image().save_png(ProjectSettings.globalize_path("res://../target/hst-22-aim.png"))
	if not _check(_matches(player.call("carry_status"), "crosshair"), "ray targets tankard: " + str(player.call("carry_status"))):
		return
	_key(true)
	await _tick(10)
	_check(not player.call("carry_status").has("grabbed_object"), "short press has not grabbed")
	_check(player.call("gameplay_reference_pose", reference_file, reference_object).visible, "hold has not picked up tankard")
	await _tick(20)
	if not _check(_matches(player.call("carry_status"), "grabbed"), "held Activate grabbed tankard"):
		return
	_check(PhysicsServer3D.body_get_mode(body) == PhysicsServer3D.BODY_MODE_RIGID, "carrying retains rigid mode")
	_check(PhysicsServer3D.body_get_collision_layer(body) != 0 and PhysicsServer3D.body_get_collision_mask(body) != 0, "carrying retains collision")
	var origin: Vector3 = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM).origin
	var player_before_wall: Vector3 = _godot(player.call("position"))
	# A server obstacle crosses the pull path while the player walks backwards.
	var wall := PhysicsServer3D.body_create()
	var shape := PhysicsServer3D.box_shape_create()
	PhysicsServer3D.shape_set_data(shape, Vector3(2.0, 2.0, 0.06))
	PhysicsServer3D.body_add_shape(wall, shape)
	PhysicsServer3D.body_set_mode(wall, PhysicsServer3D.BODY_MODE_STATIC)
	PhysicsServer3D.body_set_space(wall, root.get_world_3d().space)
	var yaw: float = player.call("facing").x
	var backward := Vector3(-sin(yaw), 0, cos(yaw))
	var wall_point := origin + backward * 0.5
	PhysicsServer3D.body_set_state(wall, PhysicsServer3D.BODY_STATE_TRANSFORM, Transform3D(Basis.looking_at(backward, Vector3.UP), wall_point))
	Input.action_press("Back")
	await _tick(60)
	Input.action_release("Back")
	var blocked: Vector3 = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM).origin
	_check((_godot(player.call("position")) - player_before_wall).dot(backward) > 0.9, "camera pulls tankard beyond collision wall")
	_check((blocked - origin).dot(backward) < 0.55, "held tankard cannot pass through collision wall")
	PhysicsServer3D.free_rid(wall)
	PhysicsServer3D.free_rid(shape)
	Input.action_press("Back")
	var travelled := 0.0
	for i in 180:
		await _tick()
		travelled = initial.distance_to(_pose().position) / UNITS_PER_METRE
		if travelled >= 2.0:
			break
	Input.action_release("Back")
	if not _check(travelled >= 2.0, "carried tankard two metres, actual=" + str(travelled)):
		return
	var velocity: Vector3 = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_LINEAR_VELOCITY)
	_key(false)
	player.call("advance", STEP)
	var released_velocity: Vector3 = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_LINEAR_VELOCITY)
	_check(not player.call("carry_status").has("grabbed_object"), "release clears shared grabbed reference")
	_check(velocity.distance_to(released_velocity) < 0.001, "release preserves momentum")
	_check(velocity.length() > 0.05, "drop was made with momentum")
	await _tick(300)
	var landed: Vector3 = _pose().position
	var ground_query := PhysicsRayQueryParameters3D.create(_godot(landed + Vector3(0, 0, 20)), _godot(landed - Vector3(0, 0, 80)))
	var movable_bodies: Array[RID] = []
	for row: Dictionary in streamer.call("movable_reference_poses"):
		movable_bodies.append(row.body)
	ground_query.exclude = movable_bodies
	var ground := root.get_world_3d().direct_space_state.intersect_ray(ground_query)
	_check(not ground.is_empty(), "tankard landed on table or floor")
	var final_velocity: Vector3 = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_LINEAR_VELOCITY)
	_check(final_velocity.length() < 0.15, "tankard settled")
	var shared: Dictionary = player.call("gameplay_reference_pose", reference_file, reference_object)
	var rendered: Dictionary = streamer.call("rendered_reference_pose", reference_file, reference_object)
	_check(shared.position.distance_to(landed) < 0.5 and rendered.position.distance_to(landed) < 0.5, "World, rendering and body agree")
	var alignment_error: float = streamer.call("render_alignment_error", reference_file, reference_object)
	_check(alignment_error >= 0.0 and alignment_error < 0.001, "uploaded tankard geometry follows body")
	await process_frame
	await process_frame
	var capture := ProjectSettings.globalize_path("res://../target/hst-22-carry.png")
	_check(get_root().get_texture().get_image().save_png(capture) == OK, "window capture written")
	# Focus loss ends a carry without an ordinary activation.
	await _approach()
	await _aim(_body_center())
	_key(true)
	await _tick(30)
	_check(_matches(player.call("carry_status"), "grabbed"), "tankard can be grabbed again")
	player.call("input_focus_lost")
	_check(not player.call("carry_status").has("grabbed_object"), "focus loss releases carry")
	_check(player.call("gameplay_reference_pose", reference_file, reference_object).visible, "interruption does not pick up tankard")
	player.call("input_focus_gained")
	await _tick(60)
	landed = _pose().position
	# A quick press uses the same input ray and performs the ordinary pickup.
	await _aim(_body_center())
	if not _check(_matches(player.call("carry_status"), "crosshair"), "tap targets dropped tankard"):
		return
	_key(true)
	await _tick()
	_key(false)
	await _tick(2)
	_check(not player.call("gameplay_reference_pose", reference_file, reference_object).visible, "tap picks up dropped tankard")
	player.free()
	streamer.free()
	if failures == 0:
		print("HST-22 PASS: tankard carried ", travelled, " metres, blocked while held, momentum preserved, landed and picked up")
	quit(0 if failures == 0 else 1)
