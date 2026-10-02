extends SceneTree

## MAG-41 host check for movable-body motion and release after interruption.
## Run with SKYRIM_DATA set after building the extension.

const CELL := "WhiterunBanneredMare"

func _initialize() -> void:
	call_deferred("mag_41_host_body_moves_and_releases_after_interruption")

func _fail(message: String) -> void:
	printerr("MAG-41 FAIL: ", message)
	quit(1)

func mag_41_host_body_moves_and_releases_after_interruption() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("CellStreamerNode"):
		_fail("installed data or extension is unavailable")
		return
	var root := Node3D.new()
	get_root().add_child(root)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	if not streamer.call("load_data", data):
		_fail("data load failed")
		return
	if streamer.call("stream_cell", CELL) <= 0 or streamer.call("stream_cell_collision", CELL) <= 0:
		_fail("Bannered Mare load failed")
		return
	var poses: Array = streamer.call("movable_reference_poses")
	if poses.is_empty():
		_fail("Bannered Mare has no movable reference body")
		return
	var held: Dictionary = poses[0]
	var file: int = held["file"]
	var object: int = held["object"]
	var body: RID = held["body"]
	var initial: Vector3 = held["position"]
	var moved := initial + Vector3(24.0, 0.0, 12.0)
	if not streamer.call("telekinesis_grab_reference", file, object):
		_fail("host body grab failed")
		return
	if not streamer.call("telekinesis_move_reference", file, object, moved):
		_fail("host body move failed")
		return
	await physics_frame
	var after_move: Array = streamer.call("movable_reference_poses")
	var moved_pose := Vector3.ZERO
	for pose: Dictionary in after_move:
		if pose["file"] == file and pose["object"] == object:
			moved_pose = pose["position"]
	if moved_pose.distance_to(moved) > 0.5:
		_fail("host body did not reach requested position: initial=" + str(initial)
			+ " moved=" + str(moved) + " observed=" + str(moved_pose))
		return
	# Focus loss interrupts an active hand cast. The host release call is the
	# cleanup operation PlayerNode uses after World clears its transient hold.
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	root.add_child(player)
	player.call("input_focus_lost")
	if not streamer.call("telekinesis_release_reference", file, object):
		_fail("interruption did not return the host body to rigid simulation")
		return
	PhysicsServer3D.body_apply_central_impulse(body, Vector3(0.0, 2.0, 0.0))
	for i in 30:
		await physics_frame
	var after_release: Array = streamer.call("movable_reference_poses")
	var released_pose := Vector3.ZERO
	for pose: Dictionary in after_release:
		if pose["file"] == file and pose["object"] == object:
			released_pose = pose["position"]
	if released_pose.distance_to(moved) < 0.1:
		_fail("released rigid body did not resume simulation")
		return
	print("RESULT: PASS mag_41 host body movement and interruption release")
	quit(0)
