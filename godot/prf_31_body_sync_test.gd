extends SceneTree

## PRF-31 checks World-backed body eligibility and changed placements.
var failures := 0

func _initialize() -> void:
	call_deferred("prf_31_body_sync_preserves_stationary_rids_and_follows_world_changes")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _nodes(node: Node) -> int:
	var count := 1
	for child: Node in node.get_children():
		count += _nodes(child)
	return count

func _poses(player: Node3D) -> void:
	for row: Dictionary in player.call("contact_actor_bodies"):
		var pose: Transform3D = PhysicsServer3D.body_get_state(row.body, PhysicsServer3D.BODY_STATE_TRANSFORM)
		_check(pose.origin.distance_to(row.position) < 0.01, "body follows World placement")

func prf_31_body_sync_preserves_stationary_rids_and_follows_world_changes() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("SKIP: PRF31 body sync requires SKYRIM_DATA")
		quit(0)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "installed data")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "interior entry")
	_check(player.call("start_gameplay"), "gameplay")
	await physics_frame
	_check(player.call("sync_contact_actors") > 1, "resident NPCs")
	var count := _nodes(root)
	var player_body: RID = player.call("contact_actor_body", 0, 0x14)
	var before: Array = player.call("contact_actor_bodies")
	_check(player.call("sample_detection", 0.0) > 0, "initial observations")
	for row: Dictionary in before:
		_check(player.call("contact_actor_body", row.file, row.object) == row.body, "stationary body keeps RID")
	_poses(player)
	var target: Dictionary = {}
	for row: Dictionary in before:
		if row.object != 0x14:
			target = row
			break
	_check(not target.is_empty(), "move target")
	if not target.is_empty():
		_check(not player.call("emit_detection_sound_at", -1, 0x14, target.file, target.object, 100), "invalid source leaves sound queue unchanged")
		_check(not player.call("emit_detection_sound_at", 0, 0x14, -1, target.object, 100), "invalid owner leaves sound queue unchanged")
		_check(player.call("emit_detection_sound_at", 0, 0x14, target.file, target.object, 100), "separate sound source and owner")
		_check(player.call("sample_detection", 0.0) > 0, "owned sound observations")
		_check(player.call("has_detection_awareness", 0, 0x14, target.file, target.object), "sound belongs to resident NPC")
		_check(player.call("queue_player_input", {"action": "move_to", "file": target.file, "object": target.object, "offset": Vector3(25, 0, 0), "match_rotation": false}), "queue placement change")
		_check(player.call("step_player_input"), "apply placement change")
		_check(player.call("sample_detection", 0.0) > 0, "moved observations")
		_poses(player)
		_check(player.call("contact_actor_body", 0, 0x14) == player_body, "moved player keeps shape RID")
	_check(player.call("set_reference_enabled", 0, 0x14, false), "disable player reference")
	_check(player.call("sample_detection", 0.0) >= 0, "disabled update")
	var disabled_body: RID = player.call("contact_actor_body", 0, 0x14)
	_check(not disabled_body.is_valid(), "disabled reference releases its body")
	_check(player.call("set_reference_enabled", 0, 0x14, true), "enable player reference")
	_check(player.call("sample_detection", 0.0) > 0, "enabled update")
	var restored_body: RID = player.call("contact_actor_body", 0, 0x14)
	_check(restored_body.is_valid(), "enabled candidate recreates its body")
	_poses(player)
	_check(_nodes(root) == count, "body updates allocate no scene nodes")
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " PRF31 body sync")
	quit(0 if failures == 0 else 1)
