extends SceneTree

## CMB-19: a World death replaces the contact capsule with Jolt ragdoll
## bodies, and resurrection returns the animated controller.

const BODY := "meshes/actors/character/character assets/malebody_1.nif"
var failures := 0


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("SKYRIM_DATA not set, skipping")
		quit(0)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"initial cell failed")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	player.call("sync_contact_actors")
	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	get_root().add_child(visual)
	_check(visual.call("load_character", data, BODY), "human rig failed to load")
	var selected: Dictionary = {}
	for row: Dictionary in player.call("contact_actor_bodies"):
		if row.file == 0 and row.object == 0x14:
			continue
		if not player.call("bind_actor_appearance", visual, row.file, row.object):
			continue
		if player.call("kill_actor", row.file, row.object):
			selected = row
			break
	_check(not selected.is_empty(), "no resident actor accepted death and appearance")
	if selected.is_empty():
		quit(1)
		return
	for i in 3:
		await process_frame
	_check(visual.call("render_mode") == "ragdoll", "death did not enter ragdoll mode")
	var bodies: Array = visual.call("ragdoll_bodies")
	_check(bodies.size() == 18, "expected 18 Jolt bodies, got %d" % bodies.size())
	for body: RID in bodies:
		_check(PhysicsServer3D.body_get_mode(body) == PhysicsServer3D.BODY_MODE_RIGID,
			"ragdoll body is not dynamic")
		_check(PhysicsServer3D.body_get_space(body) == player.get_world_3d().space,
			"ragdoll body is outside the actor's physics space")
	_check(not player.call("contact_actor_body", selected.file, selected.object).is_valid(),
		"dead actor retained its contact capsule")
	var initial_body_positions: Array[Vector3] = []
	for body: RID in bodies:
		var pose: Transform3D = PhysicsServer3D.body_get_state(
			body, PhysicsServer3D.BODY_STATE_TRANSFORM)
		initial_body_positions.append(pose.origin)
	var initial_skins: Array = visual.call("uploaded_skin_poses")
	_check(not initial_skins.is_empty(), "no rendered skin transforms")
	for i in 30:
		await physics_frame
	await process_frame
	var body_moved := false
	for i in bodies.size():
		var pose: Transform3D = PhysicsServer3D.body_get_state(
			bodies[i], PhysicsServer3D.BODY_STATE_TRANSFORM)
		body_moved = body_moved or pose.origin.distance_to(initial_body_positions[i]) > 0.0001
	_check(body_moved, "Jolt did not simulate the ragdoll")
	var skin_moved := false
	var later_skins: Array = visual.call("uploaded_skin_poses")
	_check(later_skins.size() == initial_skins.size(), "skin bone count changed during death")
	for i in mini(later_skins.size(), initial_skins.size()):
		var later: Transform3D = later_skins[i]
		var initial: Transform3D = initial_skins[i]
		skin_moved = skin_moved or later.origin.distance_to(initial.origin) > 0.0001
	_check(skin_moved, "rendered skin did not follow Jolt ragdoll bodies")
	_check(player.call("resurrect_actor", selected.file, selected.object),
		"World resurrection failed")
	for i in 3:
		await process_frame
	_check(visual.call("render_mode") == "animated", "resurrection did not restore animation")
	_check(visual.call("ragdoll_bodies").is_empty(), "resurrection retained ragdoll bodies")
	_check(player.call("contact_actor_body", selected.file, selected.object).is_valid(),
		"resurrection did not restore the contact capsule")
	visual.queue_free()
	player.queue_free()
	streamer.queue_free()
	await process_frame
	print("RESULT: %s" % ("PASS" if failures == 0 else "FAIL"))
	quit(0 if failures == 0 else 1)
