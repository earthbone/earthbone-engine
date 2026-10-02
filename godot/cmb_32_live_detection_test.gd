extends SceneTree

## CMB-32: live physics sight feeds shared detection without actor scene nodes.
var failures := 0


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func _frames(count: int) -> void:
	for i in count:
		await physics_frame


func _node_count(node: Node) -> int:
	var total := 1
	for child in node.get_children():
		total += _node_count(child)
	return total


func _make_wall(space: RID, position: Vector3, direction: Vector3) -> Dictionary:
	var shape := PhysicsServer3D.box_shape_create()
	PhysicsServer3D.shape_set_data(shape, Vector3(0.18, 1.0, 0.18))
	var body := PhysicsServer3D.body_create()
	PhysicsServer3D.body_set_mode(body, PhysicsServer3D.BODY_MODE_STATIC)
	PhysicsServer3D.body_add_shape(body, shape)
	var rotation := Basis.looking_at(direction, Vector3.UP)
	PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM,
		Transform3D(rotation, position))
	PhysicsServer3D.body_set_space(body, space)
	return {"body": body, "shape": shape}


func _free_wall(wall: Dictionary) -> void:
	PhysicsServer3D.free_rid(wall.body)
	PhysicsServer3D.free_rid(wall.shape)


func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: CMB32 requires installed data and the host extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "installed data load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data binding failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"Bannered Mare collision failed")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	await _frames(3)
	var initial_nodes := _node_count(root)
	var count: int = player.call("sync_contact_actors")
	var actors: Array = player.call("contact_actor_bodies")
	_check(count == actors.size() and count > 1, "no resident actor bodies for live sight")
	_check(_node_count(root) == initial_nodes,
		"syncing actor RIDs added a scene node")
	if actors.size() < 2:
		player.free()
		streamer.free()
		quit(1)
		return

	var initial_observations: int = player.call("sample_detection", 0.0)
	_check(initial_observations > 0, "live detector produced no ordered actor observations")
	var pair_a: Dictionary = {}
	var pair_b: Dictionary = {}
	var best_distance := INF
	var visible_value := -1.0
	for a: Dictionary in actors:
		for b: Dictionary in actors:
			if a.file == b.file and a.object == b.object:
				continue
			if not player.call("has_detection_awareness", a.file, a.object, b.file, b.object):
				continue
			var value: float = player.call("detection_value", a.file, a.object, b.file, b.object)
			var distance: float = a.position.distance_to(b.position)
			if distance < 0.4:
				continue
			var from: Vector3 = a.position + Vector3.UP * 1.4
			var to: Vector3 = b.position + Vector3.UP * 1.4
			var query := PhysicsRayQueryParameters3D.create(from, to)
			query.exclude = [a.body]
			var sight_hit: Dictionary = PhysicsServer3D.space_get_direct_state(
				player.get_world_3d().space).intersect_ray(query)
			if not sight_hit.is_empty() and sight_hit.rid != b.body:
				continue
			if distance < best_distance:
				best_distance = distance
				pair_a = a
				pair_b = b
				visible_value = value
	if pair_a.is_empty():
		_check(false, "live sample did not create shared actor awareness")
		player.free()
		streamer.free()
		quit(1)
		return
	var eye_a: Vector3 = pair_a.position + Vector3.UP * 1.4
	var eye_b: Vector3 = pair_b.position + Vector3.UP * 1.4
	var observer_id := [pair_a.file, pair_a.object]
	var target_id := [pair_b.file, pair_b.object]
	var blocker := _make_wall(player.get_world_3d().space,
		eye_a.lerp(eye_b, 0.5), eye_b - eye_a)
	await _frames(2)
	var blocked_observations: int = player.call("sample_detection", 0.0)
	var blocked_value: float = player.call("detection_value",
		observer_id[0], observer_id[1], target_id[0], target_id[1])
	var retained_before_sound: bool = player.call("has_detection_awareness",
		observer_id[0], observer_id[1], target_id[0], target_id[1])
	_check(retained_before_sound and blocked_observations > 0,
		"live sample did not retain shared actor awareness (%d observations, %.3f -> %.3f)" % [
		blocked_observations, visible_value, blocked_value])
	_check(blocked_value < visible_value,
		"a PhysicsServer wall did not lower live sight detection (%.3f to %.3f)" % [visible_value, blocked_value])
	_check(player.call("emit_detection_sound", target_id[0], target_id[1], 100),
		"target actor did not emit a detection event")
	player.call("sample_detection", 0.0)
	var heard_value: float = player.call("detection_value",
		observer_id[0], observer_id[1], target_id[0], target_id[1])
	_check(heard_value > blocked_value,
		"the emitted sound did not change shared detection (%.3f to %.3f)" % [blocked_value, heard_value])
	print("CMB32 live sight: actors=%d pair=%x/%x distance=%.3fm visible=%.3f blocked=%.3f heard=%.3f" % [
		count, pair_a.object, pair_b.object, best_distance, visible_value, blocked_value, heard_value])
	_free_wall(blocker)
	await _frames(2)

	var base_nodes := _node_count(root)
	var started := Time.get_ticks_usec()
	for i in 40:
		player.call("sync_contact_actors")
		player.call("sample_detection", 1.0 / 60.0)
	var elapsed := Time.get_ticks_usec() - started
	_check(_node_count(root) == base_nodes,
		"repeated observation allocated per-actor scene nodes")
	var pair_count := count * (count - 1)
	print("CMB32 host cost: actors=%d ordered_pairs=%d samples=40 elapsed_us=%d us_per_pair_sample=%.3f scene_node_delta=0" % [
		count, pair_count, elapsed, float(elapsed) / (40.0 * pair_count)])

	var old_cell: String = player.call("gameplay_cell")
	var doors: Array = player.call("gameplay_doors")
	var door: Dictionary = {}
	for candidate: Dictionary in doors:
		if not candidate.locked:
			door = candidate
			break
	_check(not door.is_empty(), "no unlocked door for cell transition")
	if not door.is_empty():
		_check(player.call("activate_reference", door.file, door.object), "door transition failed")
		await _frames(3)
		var new_cell: String = player.call("gameplay_cell")
		_check(new_cell != old_cell, "door did not change the active cell")
		var retained: bool = player.call("has_detection_awareness",
			observer_id[0], observer_id[1], target_id[0], target_id[1])
		var retained_value: float = player.call("detection_value",
			observer_id[0], observer_id[1], target_id[0], target_id[1])
		_check(retained, "cell change discarded awareness keyed by actor FormIDs")
		var new_count: int = player.call("sync_contact_actors")
		var new_nodes := _node_count(root)
		_check(new_nodes == base_nodes, "cell replacement added per-actor scene nodes")
		print("CMB32 cell change: %s -> %s retained=%x/%x awareness=%.3f actors=%d nodes=%d" % [
			old_cell, new_cell, pair_a.object, pair_b.object, retained_value, new_count, new_nodes])

	player.free()
	streamer.free()
	await _frames(2)
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " CMB32 live detection")
	quit(0 if failures == 0 else 1)
