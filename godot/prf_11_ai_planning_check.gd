extends SceneTree

## PRF-11: 60 simulated seconds of installed Bannered Mare NPC planning.


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("SKIP PRF-11: SKYRIM_DATA is unset")
		quit(0)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	var loaded: bool = streamer.call("load_data", data)
	loaded = loaded and player.call("use_streamer_data", streamer)
	loaded = loaded and player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0
	loaded = loaded and player.call("start_gameplay")
	loaded = loaded and player.call("prepare_ai_planning_check")
	if not loaded:
		printerr("FAIL PRF-11: gameplay startup")
		player.free()
		streamer.free()
		quit(1)
		return
	var actors: int = player.call("sync_contact_actors")
	var worst := 0.0
	var failures := 0
	for frame in 3600:
		player.call("advance", 1.0 / 60.0)
		var planning: float = player.call("ai_planning_microseconds")
		worst = maxf(worst, planning)
		if planning > 8000.0:
			failures += 1
			if failures <= 3:
				printerr("FAIL PRF-11: frame %d AI planning %.3f us" % [frame, planning])
		if frame % 600 == 0:
			print("PRF-11 phases frame=%d " % frame, player.call("ai_planning_details"))
		if frame % 60 == 0:
			await process_frame
	var evaluations: int = player.call("ai_package_evaluations")
	if actors < 5 or evaluations < actors:
		failures += 1
		printerr("FAIL PRF-11: NPC planning was not exercised")
	print("PRF-11 actors=%d frames=3600 seconds=60 evaluations=%d worst_ai_us=%.3f failures=%d" % [
		actors, evaluations, worst, failures])
	player.free()
	streamer.free()
	quit(0 if failures == 0 else 1)
