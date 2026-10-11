extends SceneTree

## MAG-66 runtime check for hold, throw and effect-expiry interruption.
const CELL := "WhiterunBanneredMare"
const PLAYER := 0x14

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("MAG-66 FAIL: ", message)

func _camera_for(player: Node) -> Camera3D:
	for child: Node in player.get_children():
		if child is Camera3D:
			return child
	return null

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		_check(false, "installed data or host extension is unavailable")
		quit(1)
		return
	var root := Node3D.new()
	get_root().add_child(root)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "installed data load failed")
	_check(streamer.call("stream_cell", CELL) > 0, "Bannered Mare load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", true)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "player did not share installed data")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0, "player cell load failed")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	if failures > 0:
		quit(1)
		return
	for _frame in 120:
		await physics_frame

	var spells: Array = player.call("grab_actor_spells")
	var spell: Dictionary = {}
	for candidate: Dictionary in spells:
		if int(candidate.get("duration", 0)) > 0 \
			and int(candidate.get("delivery", -1)) == 0 \
			and int(candidate.get("cast_type", -1)) == 2:
			spell = candidate
			break
	_check(not spell.is_empty(), "installed load order has no finite targeted Grab Actor spell")
	if spell.is_empty():
		quit(1)
		return

	var target: Dictionary = {}
	var actors: Array = player.call("contact_actor_bodies")
	var camera := _camera_for(player)
	_check(camera != null, "PlayerNode camera is unavailable")
	if camera == null:
		quit(1)
		return
	var best_distance := INF
	for row: Dictionary in actors:
		if int(row.file) == 0 and int(row.object) == PLAYER:
			continue
		var distance: float = camera.global_position.distance_to(row.position)
		if distance < best_distance:
			best_distance = distance
			target = row
	_check(not target.is_empty(), "Bannered Mare has no resident NPC body")
	if target.is_empty():
		quit(1)
		return

	var body: RID = target.body
	var initial: Vector3 = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM).origin
	_check(player.call("cast_grab_actor_spell_at_actor", int(spell.file), int(spell.object),
		int(target.file), int(target.object)), "targeted Grab Actor spell did not cast")
	for _frame in 3:
		await physics_frame
	var held: Vector3 = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM).origin
	_check(held.distance_to(initial) > 0.25, "active effect did not move the actor into its held pose")

	for _frame in int(spell.duration * 60.0) + 30:
		await physics_frame
	_check(PhysicsServer3D.body_get_mode(body) == PhysicsServer3D.BODY_MODE_RIGID,
		"effect expiry did not release the held actor body")
	var velocity: Vector3 = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_LINEAR_VELOCITY)
	_check(velocity.length() > 0.1, "released actor did not receive a throw impulse")

	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " MAG-66 hold, throw and expiry interruption")
	quit(0 if failures == 0 else 1)
