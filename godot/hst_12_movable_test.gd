extends SceneTree

## HST-12 live check against the installed Bannered Mare records.
## Run with SKYRIM_DATA set after building the release extension.

const CELL := "WhiterunBanneredMare"
var streamer: Node3D
var player: Node3D

func _initialize() -> void:
	call_deferred("_run")

func _fail(message: String) -> void:
	printerr("HST-12 FAIL: ", message)
	quit(1)

func _tick() -> void:
	await physics_frame
	player.call("advance", 1.0 / 60.0)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		_fail("installed data or extension is unavailable")
		return
	var root := Node3D.new()
	get_root().add_child(root)
	streamer = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	if not streamer.call("load_data", data):
		_fail("data load failed")
		return
	if streamer.call("stream_cell", CELL) <= 0 or streamer.call("stream_cell_collision", CELL) <= 0:
		_fail("cell stream failed")
		return
	var bodies: Array = streamer.call("movable_reference_poses")
	if bodies.is_empty():
		_fail("no authored movable body")
		return
	player = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	if not player.call("use_streamer_data", streamer):
		_fail("source share failed")
		return
	if player.call("enter_cell", CELL, 0.0, 0.0, 500.0) <= 0 or not player.call("start_gameplay"):
		_fail("game start failed")
		return
	var selected: Dictionary = bodies[0]
	var file: int = selected["file"]
	var object: int = selected["object"]
	var body: RID = selected["body"]
	var initial: Vector3 = selected["position"]
	PhysicsServer3D.body_apply_central_impulse(body, Vector3(0.0, 4.0, 0.0))
	var max_motion := 0.0
	for i in 180:
		await _tick()
		for row in streamer.call("movable_reference_poses"):
			if row["file"] == file and row["object"] == object:
				max_motion = max(max_motion, initial.distance_to(row["position"]))
	var world: Dictionary = player.call("gameplay_reference_pose", file, object)
	var drawn: Dictionary = streamer.call("rendered_reference_pose", file, object)
	var latest: Array = streamer.call("movable_reference_poses")
	var simulated := Vector3.ZERO
	for row in latest:
		if row["file"] == file and row["object"] == object:
			simulated = row["position"]
	if world.is_empty() or drawn.is_empty():
		_fail("missing shared or drawn pose")
		return
	if max_motion < 1.0:
		_fail("rigid body did not move")
		return
	if simulated.z < initial.z - 500.0:
		_fail("rigid body fell through the cell collision")
		return
	if world["position"].distance_to(simulated) > 0.5 or drawn["position"].distance_to(simulated) > 0.5:
		_fail("body, World and rendering disagree")
		return
	var render_error: float = streamer.call("render_alignment_error", file, object)
	if render_error < 0.0 or render_error > 0.001:
		_fail("uploaded geometry differs from World")
		return
	var count: int = streamer.call("collision_stats")["movable"]
	if not player.call("set_reference_enabled", file, object, false):
		_fail("disable failed")
		return
	await _tick()
	if streamer.call("collision_stats")["movable"] != count - 1:
		_fail("disable did not free the body RID")
		return
	if not player.call("set_reference_enabled", file, object, true):
		_fail("re-enable failed")
		return
	await _tick()
	if streamer.call("collision_stats")["movable"] != count:
		_fail("re-enable did not restore the body")
		return
	if not player.call("despawn_reference", file, object):
		_fail("despawn failed")
		return
	await _tick()
	if streamer.call("collision_stats")["movable"] != count - 1:
		_fail("despawn did not free the body")
		return
	streamer.call("clear")
	if streamer.call("collision_stats")["bodies"] != 0:
		_fail("cell unload left bodies live")
		return
	print("RESULT: PASS hst_12 movable physics")
	quit(0)
