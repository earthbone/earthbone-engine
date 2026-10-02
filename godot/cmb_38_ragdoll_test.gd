extends SceneTree

## CMB-38: queued impulses reach Jolt ragdolls and explicit modes toggle them.

const BODY := "meshes/actors/character/character assets/malebody_1.nif"
const PLAYER := 0x14
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
		if row.file == 0 and row.object == PLAYER:
			continue
		if player.call("bind_actor_appearance", visual, row.file, row.object):
			selected = row
			break
	_check(not selected.is_empty(), "no resident actor accepted an appearance")
	if selected.is_empty():
		quit(1)
		return
	var actor_file: int = selected.file
	var actor_object: int = selected.object
	_check(player.call("force_actor_ragdoll", actor_file, actor_object, true),
		"ForceAddRagdollToWorld state update failed")
	for i in 5:
		await process_frame
	var bodies: Array = visual.call("ragdoll_bodies")
	_check(bodies.size() == 18, "explicit add did not install 18 Jolt bodies")
	if bodies.is_empty():
		quit(1)
		return
	var root_body: RID = bodies[0]
	var before: Vector3 = PhysicsServer3D.body_get_state(
		root_body, PhysicsServer3D.BODY_STATE_LINEAR_VELOCITY)
	_check(player.call("push_actor_away", 0, PLAYER, actor_file,
		actor_object, 50.0), "PushActorAway did not queue its impulse")
	for i in 5:
		await physics_frame
	var after: Vector3 = PhysicsServer3D.body_get_state(
		root_body, PhysicsServer3D.BODY_STATE_LINEAR_VELOCITY)
	_check(after.distance_to(before) > 0.0001,
		"queued actor impulse did not change Jolt ragdoll velocity")
	before = after
	_check(player.call("knock_area_effect", 0, PLAYER, 1.0, 10000.0),
		"KnockAreaEffect did not queue area impulses")
	for i in 5:
		await physics_frame
	after = PhysicsServer3D.body_get_state(
		root_body, PhysicsServer3D.BODY_STATE_LINEAR_VELOCITY)
	_check(after.distance_to(before) > 0.0001,
		"area impulse did not change Jolt ragdoll velocity")
	_check(player.call("force_actor_ragdoll", actor_file, actor_object, false),
		"ForceRemoveRagdollFromWorld state update failed")
	for i in 5:
		await process_frame
	_check(visual.call("ragdoll_bodies").is_empty(),
		"explicit remove retained Jolt ragdoll bodies")
	_check(visual.call("render_mode") == "animated",
		"explicit remove did not restore animated rendering")
	visual.queue_free()
	player.queue_free()
	streamer.queue_free()
	await process_frame
	print("RESULT: %s" % ("PASS" if failures == 0 else "FAIL"))
	quit(0 if failures == 0 else 1)
