extends SceneTree

var failures := 0


func _initialize() -> void:
	if not ClassDB.class_exists("PlayerNode"):
		printerr("PRF-18 FAIL: PlayerNode is not registered")
		quit(1)
		return
	call_deferred("prf_18_host_slow_frame_keeps_five_seconds_and_writes_user_hitches")


func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-18 FAIL: ", message)


func prf_18_host_slow_frame_keeps_five_seconds_and_writes_user_hitches() -> void:
	var directory := "user://hitches"
	check(DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(directory)) == OK, "could not create hitch directory")
	var before := DirAccess.get_files_at(directory)
	before.sort()
	var seen := before
	Engine.max_fps = 60
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	var start := Time.get_ticks_usec()
	var eligible := start + 5_200_000
	while Time.get_ticks_usec() < eligible:
		player.call("advance", 1.0 / 60.0)
		await process_frame
		var files := DirAccess.get_files_at(directory)
		files.sort()
		if files != seen:
			seen = files
			eligible = Time.get_ticks_usec() + 10_100_000
		if Time.get_ticks_usec() - start > 60_000_000:
			check(false, "startup dump cooldown did not finish")
			player.free()
			quit(1)
			return
	OS.delay_msec(80)
	await process_frame
	start = Time.get_ticks_usec()
	while Time.get_ticks_usec() - start < 1_200_000:
		player.call("advance", 1.0 / 60.0)
		await process_frame
	player.free()
	var found := false
	for filename in DirAccess.get_files_at(directory):
		if not filename.ends_with(".json") or filename in before:
			continue
		var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(directory + "/" + filename))
		check(parsed is Dictionary and parsed.has("traceEvents"), "invalid Chrome trace JSON")
		if not parsed is Dictionary or not parsed.has("traceEvents"):
			continue
		var frames_before := 0
		var earliest_frame := INF
		var latest_frame_end := 0.0
		var has_tail := false
		var has_counter := false
		var has_slow_frame := false
		var longest: Dictionary = {}
		for event: Dictionary in parsed.traceEvents:
			if event.ph == "X" and event.name != "frame" and event.dur > 0.0:
				longest[event.name] = maxf(longest.get(event.name, 0.0), event.dur)
			if event.name == "frame":
				earliest_frame = minf(earliest_frame, event.ts)
				latest_frame_end = maxf(latest_frame_end, event.ts + event.dur)
				if event.ts < parsed.hitch.trigger_us - 80_000.0:
					frames_before += 1
				if event.ts > parsed.hitch.trigger_us + 900_000.0:
					has_tail = true
				if event.dur >= 75_000.0:
					has_slow_frame = true
			if event.ph == "C" and event.name == "raycasts":
				has_counter = true
		if has_slow_frame and frames_before >= 200 and has_tail and earliest_frame <= 50_000.0 and latest_frame_end >= parsed.hitch.trigger_us + 999_999.0:
			found = true
			check(has_counter, "missing counter events")
			check(parsed.hitch.overwritten_events == 0, "history overflowed")
			var summary := FileAccess.get_file_as_string(directory + "/" + filename.trim_suffix(".json") + ".txt")
			check(summary.strip_edges().split("\n").size() == 1, "summary is not one line")
			var names: Array = longest.keys()
			names.sort_custom(func(left: String, right: String) -> bool:
				if longest[left] == longest[right]:
					return left < right
				return longest[left] > longest[right])
			for name: String in names.slice(0, 3):
				check(summary.contains(name + "="), "summary missing slowest span " + name)
	check(found, "no dump retained five seconds before and one second after the forced hitch")
	print("prf_18_host_slow_frame_keeps_five_seconds_and_writes_user_hitches: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
