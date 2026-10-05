extends SceneTree

## PRF-22 replays fixed-step input through the real PlayerNode transport.
var failures := 0

func _initialize() -> void:
	call_deferred("prf_22_host_records_600_frames_and_replays_twice")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL prf_22: ", message)

func prf_22_host_records_600_frames_and_replays_twice() -> void:
	var data := OS.get_environment("SOE_PLY_01_FIXTURE")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL prf_22: exported PLY-01 fixture and extension required")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "fixture load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared records")
	player.call("enter_cell", "PlyStart", 0.0, 0.0, 0.0)
	_check(player.call("start_gameplay"), "Game start")
	_check(player.call("record_player_session", 123456), "start recording")
	var hashes: PackedStringArray = [player.call("player_session_hash")]
	for frame in range(600):
		var command := {"action": "wait"}
		if frame % 100 == 0:
			command = {"action": "activate", "file": 0, "object": 0x810}
		elif frame % 100 == 1:
			command = {"action": "turn", "rotation": Vector3(0, 0, float(frame) / 600.0)}
		_check(player.call("queue_player_input", command), "queue frame " + str(frame))
		_check(player.call("step_player_input"), "record frame " + str(frame))
		if (frame + 1) % 60 == 0:
			hashes.append(player.call("player_session_hash"))
	var path := data.path_join("host-session.soer")
	_check(player.call("write_player_session", path), "write recording")
	var events: PackedStringArray = player.call("player_input_log")
	for run in range(2):
		var restored: bool = player.call("replay_player_session", path)
		_check(restored, "restore run " + str(run))
		if not restored:
			break
		_check(player.call("player_session_hash") == hashes[0], "initial hash")
		for frame in range(600):
			var stepped: bool = player.call("step_player_input")
			_check(stepped, "replay run %d frame %d" % [run, frame])
			if not stepped:
				break
			if (frame + 1) % 60 == 0:
				_check(player.call("player_session_hash") == hashes[(frame + 1) / 60], "60-frame hash")
		_check(player.call("player_session_error").is_empty(), "no replay divergence")
		_check(player.call("player_input_log") == events, "same outcomes and Papyrus traces")
		_check(player.call("player_input_pending") == 0, "queue drained")
	print("prf_22_host_records_600_frames_and_replays_twice: ", "PASS" if failures == 0 else "FAIL", " checkpoints=", hashes.size())
	player.free()
	streamer.free()
	quit(0 if failures == 0 else 1)
