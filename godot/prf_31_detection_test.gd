extends SceneTree

## PRF-31 measures the complete detection entry point, including body sync.
const DETECTION_BUDGET_US := 2000
var failures := 0

func _initialize() -> void:
	call_deferred("prf_31_gate_walk_keeps_detection_under_two_ms")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		if failures <= 10:
			printerr("FAIL: ", message)

func prf_31_gate_walk_keeps_detection_under_two_ms() -> void:
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "data load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "entry")
	_check(player.call("start_gameplay"), "gameplay")
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			_check(player.call("activate_reference", door.file, door.object), "exterior door")
			break
	player.call("advance", 0.0)
	await physics_frame
	var actors: int = player.call("sync_contact_actors")
	var started := Time.get_ticks_usec()
	var pairs: int = player.call("sample_detection", 1.0 / 60.0)
	var elapsed := Time.get_ticks_usec() - started
	print("PRF31 exterior cell=", player.call("gameplay_cell"), " actors=", actors, " pairs=", pairs, " detection_us=", elapsed)
	print("PRF31 stages sync/light/observations/evaluation=", player.call("detection_stage_times"))
	print("PRF31 resident body placements=", player.call("contact_actor_bodies"))
	if OS.get_environment("PRF_31_BASELINE") != "1":
		_check(elapsed <= DETECTION_BUDGET_US, "detection exceeded 2 ms")
		await _walk_gate(player)
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " PRF31 detection")
	quit(0 if failures == 0 else 1)

func _walk_gate(player: Node3D) -> void:
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "gate links to Tamriel")
	if gate.is_empty():
		return
	_check(player.call("detection_worldspace") == "WhiterunWorld", "Whiterun worldspace")
	_check(player.call("queue_player_input", {"action": "move_to", "file": gate.file, "object": gate.object, "offset": Vector3(0, 200, 0), "match_rotation": true}), "position near city gate")
	_check(player.call("step_player_input"), "city gate placement")
	player.call("advance", 0.0)
	var city_start: Vector3 = player.call("position")
	var tamriel_start := Vector3.ZERO
	var maximum_us := 0
	var over_budget := 0
	var cells := {}
	var sample_count := 0
	var stage_totals := PackedInt64Array([0, 0, 0, 0])
	var frame := 0
	var city_movement := 0.0
	var tamriel_movement := 0.0
	for phase in range(2):
		if phase == 1:
			_check(city_movement > 100, "walked inside Whiterun")
			_check(player.call("activate_reference", gate.file, gate.object), "Whiterun gate crossing")
			_check(player.call("detection_worldspace") == "Tamriel", "Tamriel worldspace")
			player.call("advance", 0.0)
			tamriel_start = player.call("position")
		_check(player.call("sync_contact_actors") > 1, "resident NPCs in phase " + str(phase))
		print("PRF31 phase=", phase, " resident body placements=", player.call("contact_actor_bodies"))
		var owner: Dictionary = {}
		for row: Dictionary in player.call("contact_actor_bodies"):
			if not (row.file == 0 and row.object == 0x14) and not row.dead:
				owner = row
				break
		_check(not owner.is_empty(), "living resident sound owner")
		if owner.is_empty():
			return
		_check(player.call("emit_detection_sound_at", 0, 0x14, owner.file, owner.object, 100), "warm actor-owned sound")
		_check(player.call("sample_detection", 0.0) > 0, "warm resident observations")
		var started := Time.get_ticks_usec()
		var phase_samples := 0
		var phase_pairs := 0
		var phase_stages := PackedInt64Array([0, 0, 0, 0])
		var facing: Vector2 = player.call("facing")
		while Time.get_ticks_usec() - started < 30000000:
			var seconds := float(Time.get_ticks_usec() - started) / 1000000.0
			var sign := 1.0 if int(seconds / 5.0) % 2 == 0 else -1.0
			await physics_frame
			var direction := Vector3(sin(facing.x), cos(facing.x), 0) * sign
			_check(player.call("emit_detection_sound_at", 0, 0x14, owner.file, owner.object, 100), "queued actor-owned sound")
			var pairs: int = player.call("sample_detection_walk", 1.0 / 60.0, direction)
			phase_pairs += maxi(pairs, 0)
			var detection_us: int = player.call("detection_time_usec")
			_check(pairs >= 0, "detection ran on frame " + str(frame))
			if detection_us > DETECTION_BUDGET_US:
				over_budget += 1
			maximum_us = maxi(maximum_us, detection_us)
			sample_count += 1
			var stages: PackedInt64Array = player.call("detection_stage_times")
			for stage in range(4):
				stage_totals[stage] += stages[stage]
				phase_stages[stage] += stages[stage]
			cells[player.call("gameplay_cell")] = true
			var position: Vector3 = player.call("position")
			if phase == 0:
				city_movement = maxf(city_movement, Vector2(position.x, position.y).distance_to(Vector2(city_start.x, city_start.y)))
			else:
				tamriel_movement = maxf(tamriel_movement, Vector2(position.x, position.y).distance_to(Vector2(tamriel_start.x, tamriel_start.y)))
			if frame % 600 == 0 or phase_samples == 0:
				var actors: int = player.call("sync_contact_actors")
				_check(actors > 1, "resident NPCs on frame " + str(frame))
				print("PRF31 walk frame=", frame, " world=", player.call("detection_worldspace"), " cell=", player.call("gameplay_cell"), " actors=", actors, " pairs=", pairs, " detection_us=", detection_us)
				print("PRF31 stages sync/light/observations/evaluation=", stages)
			frame += 1
			phase_samples += 1
		print("PRF31 phase=", phase, " seconds=", float(Time.get_ticks_usec() - started) / 1000000.0, " samples=", phase_samples, " stage_totals=", phase_stages)
		print("PRF31 phase=", phase, " feet=", player.call("position"), " pairs=", phase_pairs)
		_check(phase_pairs > 0, "resident actors produced observations in phase " + str(phase))
	_check(tamriel_movement > 100, "walked in Tamriel")
	_check(cells.size() >= 2, "walk visited Whiterun and Tamriel cells")
	_check(over_budget == 0, "detection frames over 2 ms: " + str(over_budget))
	print("PRF31 walk seconds=60 samples=", sample_count, " cells=", cells.size(), " maximum_us=", maximum_us, " over_budget=", over_budget, " city_movement=", city_movement, " Tamriel_movement=", tamriel_movement)
	print("PRF31 stage totals sync/light/observations/evaluation=", stage_totals)
