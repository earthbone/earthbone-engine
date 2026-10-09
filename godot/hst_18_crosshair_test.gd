extends SceneTree

## HST-18 installed host probe for live crosshair and grabbed Papyrus globals.

const UNITS_PER_METRE := 69.99125
const STEP := 1.0 / 60.0
var player: Node3D
var streamer: Node3D
var reference_file: int
var reference_object: int
var failures := 0

func _initialize() -> void:
	call_deferred("hst_18_live_reference_natives")

func _check(ok: bool, message: String) -> bool:
	if not ok:
		printerr("HST-18 FAIL: ", message)
		failures += 1
		quit(1)
	return ok

func _tick(count := 1) -> void:
	for i in count:
		await physics_frame
		player.call("advance", STEP)

func _matches(value: Dictionary) -> bool:
	return value.get("file", -1) == reference_file and value.get("object", -1) == reference_object

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

func _body_center(body: RID) -> Vector3:
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

func _aim(point: Vector3) -> void:
	var eye := _skyrim(get_root().get_camera_3d().global_position)
	var direction := point - eye
	var yaw := atan2(direction.x, direction.y)
	var pitch := atan2(direction.z, Vector2(direction.x, direction.y).length())
	_check(player.call("queue_player_input", {"action": "turn", "rotation": Vector3(-pitch, 0, yaw)}), "turn queued")
	_check(player.call("step_player_input"), "turn applied")
	await _tick()

func hst_18_live_reference_natives() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("HST-18 SKIP: SKYRIM_DATA unset")
		quit(0)
		return
	reference_file = int(OS.get_environment("SOE_HST_18_FILE"))
	reference_object = int(OS.get_environment("SOE_HST_18_OBJECT"))
	var root := Node3D.new()
	get_root().add_child(root)
	streamer = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	if not _check(streamer.call("load_data", data), "data load"):
		return
	if not _check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0 and streamer.call("stream_cell_collision", "WhiterunBanneredMare") > 0, "cell stream"):
		return
	var target := {}
	for row: Dictionary in streamer.call("movable_reference_poses"):
		if row.file == reference_file and row.object == reference_object:
			target = row
			break
	if not _check(not target.is_empty(), "dynamic tankard body"):
		return
	var spawn: Vector3 = target.position + Vector3(-85, 0, -10)
	var floor_query := PhysicsRayQueryParameters3D.create(_godot(spawn), _godot(spawn - Vector3(0, 0, 500)))
	var floor_hit := root.get_world_3d().direct_space_state.intersect_ray(floor_query)
	if not _check(not floor_hit.is_empty(), "floor below tankard level"):
		return
	spawn.z = _skyrim(floor_hit.position).z + 5.0
	player = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	if not _check(player.call("use_streamer_data", streamer), "shared source"):
		return
	if not _check(player.call("enter_cell", "WhiterunBanneredMare", spawn.x, spawn.y, spawn.z) > 0 and player.call("start_gameplay"), "gameplay start"):
		return
	await _tick(60)
	var body: RID = target.body
	var center := _body_center(body)
	await _aim(center)
	for frame in 3:
		var reference: Dictionary = player.call("debug_reference_native", "GetCurrentCrosshairRef")
		if not _check(_matches(reference), "Papyrus crosshair getter observes host ray on frame " + str(frame) + ": " + str(reference) + ", host=" + str(player.call("carry_status"))):
			return
		await _tick()
	var before_hold: Vector3 = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM).origin
	_key(true)
	await _tick(30)
	if not _check(_matches(player.call("debug_reference_native", "GetPlayerGrabbedRef")), "Papyrus grabbed getter observes host hold"):
		return
	Input.action_press("Back")
	await _tick(60)
	Input.action_release("Back")
	var while_held: Vector3 = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM).origin
	if not _check(while_held.distance_to(before_hold) > 1.0, "held tankard moves under host physics"):
		return
	_key(false)
	await _tick()
	_check(player.call("debug_reference_native", "GetPlayerGrabbedRef").is_empty(), "Papyrus grabbed getter clears on release")
	_check(PhysicsServer3D.body_get_mode(body) == PhysicsServer3D.BODY_MODE_RIGID, "released tankard remains simulated")
	_check(player.call("debug_reference_native", "GetCurrentCrosshairRef").is_empty() or _matches(player.call("debug_reference_native", "GetCurrentCrosshairRef")), "crosshair getter returns a live hit or None after release")
	player.free()
	streamer.free()
	if failures == 0:
		print("HST-18 PASS: per-frame crosshair, held physics reference, release and both Papyrus getters")
	quit(0 if failures == 0 else 1)
