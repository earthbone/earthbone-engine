extends SceneTree

var failures := 0

func _initialize() -> void:
	call_deferred("run")

func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-20 FAIL: " + message)

func run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("PRF-20 SKIP: SKYRIM_DATA unset")
		quit(0)
		return
	if not ClassDB.class_exists("CellStreamerNode"):
		printerr("PRF-20 FAIL: extension did not load")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	check(streamer.call("load_data", data), "load data")
	for cell in ["WhiterunBanneredMare", "WhiterunOrigin"]:
		streamer.call("clear")
		var built: int = streamer.call("stream_cell", cell) if cell == "WhiterunBanneredMare" else streamer.call("stream_exterior", cell, 0)
		check(built > 0, cell + " meshes")
		check(streamer.call("stream_cell_collision", cell) > 0, cell + " collision")
		var profiles: Dictionary = streamer.call("streaming_profiles")
		check(profiles.size() == 1, cell + " one cell report")
		for profile: Dictionary in profiles.values():
			for stage in ["archive_io", "nif_parse", "texture_decode", "texture_upload", "collision_build", "navmesh"]:
				check(profile.stage_ns.has(stage), cell + " " + stage)
			check(profile.stage_ns.texture_upload > 0, cell + " actual upload time")
			check(not profile.slowest_asset.is_empty(), cell + " slowest asset")
			check(str(streamer.call("streaming_diagnostic")).contains(profile.slowest_asset), cell + " diagnostic asset")
			var total := 0
			for bytes: int in profile.texture_bytes.values():
				total += bytes
			check(total == profile.texture_total and total > 0, cell + " texture sum")
		for profile: Dictionary in profiles.values():
			print("PRF-20 ", cell, " stages=", profile.stage_ns, " texture_bytes=", profile.texture_total, " slowest=", profile.slowest_asset)
	var plugins: Dictionary = streamer.call("plugin_record_memory")
	check(plugins.size() == 5, "five master record reports")
	for bytes: int in plugins.values():
		check(bytes > 0, "nonempty plugin record bodies")
	streamer.call("clear")
	check(streamer.call("streaming_profiles").is_empty(), "released cells leave no profile")
	streamer.free()
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	get_root().add_child(player)
	await process_frame
	await process_frame
	var memory: Dictionary = player.call("system_frame_breakdown")
	check(memory.size() == 14, "all system memory slots")
	for slot: Dictionary in memory.values():
		check(slot.has("live_bytes") and slot.has("allocations"), "frame memory fields")
		check(slot.live_bytes >= 0 and slot.allocations >= 0, "nonnegative memory counters")
	player.free()
	print("RESULT: PASS" if failures == 0 else "RESULT: FAIL")
	quit(0 if failures == 0 else 1)
