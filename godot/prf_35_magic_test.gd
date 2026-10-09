extends "res://walkthrough.gd"

## PRF-35 uses the actual windowed PRF-12 walkthrough with fixed magic budgets.
const SAMPLES := 360
const DT := 1.0 / 60.0
const P50_MS := 0.2
const P99_MS := 0.5
var failures := 0
var results := {}


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)


func _process(_delta: float) -> bool:
	return false


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-35 FAIL: ", message)


func _start() -> void:
	if OS.get_environment("SKYRIM_DATA").is_empty():
		print("prf_35_installed_windowed_magic_budget: SKIP SKYRIM_DATA unset")
		quit(0)
		return
	_check(DisplayServer.get_name() != "headless", "windowed rendering required")
	await super._start()
	if not loaded:
		quit(1)
		return
	_configure_player()
	_check(player.call("prepare_ai_planning_check"), "navigation prepared")
	await _measure("bannered_mare")
	var exited := false
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			exited = player.call("activate_reference", door.file, door.object)
			break
	_check(exited, "authored Bannered Mare exit")
	player.call("advance", 0.0)
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "Whiterun gate found")
	if not gate.is_empty():
		_check(player.call("queue_player_input", {"action": "move_to", "file": gate.file, "object": gate.object, "offset": Vector3(0, 200, 0), "match_rotation": true}), "gate approach")
		_check(player.call("step_player_input"), "gate approach applied")
		_check(player.call("activate_reference", gate.file, gate.object), "authored gate crossing")
		player.call("advance", 0.0)
	_check(player.call("detection_worldspace") == "Tamriel", "Whiterun plains worldspace")
	await _measure("whiterun_plains")
	if OS.get_environment("SOE_PRF_35_BASELINE") != "1":
		_prf_35_light_hook_retains_vanilla_expiry()
	var path := OS.get_environment("SOE_PRF_35_REPORT")
	if not path.is_empty():
		var file := FileAccess.open(path, FileAccess.WRITE)
		_check(file != null, "report opened")
		if file != null:
			file.store_string(JSON.stringify(results, "\t"))
	print(player.call("system_summary"))
	print("prf_35_installed_windowed_magic_budget: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)


func _measure(location: String) -> void:
	for i in 30:
		player.call("advance", DT)
		await process_frame
	var start: Vector3 = player.call("position")
	var magic: Array[float] = []
	var frames: Array[float] = []
	var phases := {}
	var raw: Array[Dictionary] = []
	var runtime_start: Dictionary = player.call("budget_runtime_status")
	var moved := 0.0
	for i in SAMPLES:
		player.call("input_focus_gained")
		var event := InputEventKey.new()
		event.physical_keycode = KEY_W if i < SAMPLES / 2 else KEY_S
		event.keycode = event.physical_keycode
		event.pressed = true
		Input.parse_input_event(event)
		Input.flush_buffered_events()
		_check(player.call("begin_budget_frame"), "frame window opened")
		player.call("advance", DT)
		var sample: Dictionary = player.call("end_budget_frame")
		magic.append(sample.systems.magic)
		frames.append(sample.frame_ms)
		raw.append(sample)
		if sample.has("magic_phases"):
			for phase in sample.magic_phases:
				if not phases.has(phase):
					phases[phase] = []
				phases[phase].append(sample.magic_phases[phase])
		var feet: Vector3 = player.call("position")
		moved = maxf(moved, Vector2(feet.x, feet.y).distance_to(Vector2(start.x, start.y)))
		event.pressed = false
		Input.parse_input_event(event)
		Input.flush_buffered_events()
		await process_frame
		if i % 120 == 119:
			print("PRF-35 progress ", location, " ", i + 1)
	magic.sort()
	frames.sort()
	var row := {"samples": magic.size(), "p50_ms": magic[179], "p99_ms": magic[356], "worst_ms": magic[-1], "frame_p50_ms": frames[179], "movement": moved, "phases": {}, "raw": raw}
	for phase in phases:
		phases[phase].sort()
		row.phases[phase] = {"p50_ms": phases[phase][179], "p99_ms": phases[phase][356], "worst_ms": phases[phase][-1]}
	var runtime_end: Dictionary = player.call("budget_runtime_status")
	row["active_effects"] = runtime_end.get("magic_active_effects", 0)
	_check(runtime_end.ticks - runtime_start.ticks == SAMPLES, "one tick per sample")
	_check(runtime_end.evaluations > runtime_start.evaluations, "NPC packages ran")
	_check(moved > 32.0, "player walked in " + location)
	results[location] = row
	print("PRF-35 ", location, " samples=", row.samples, " p50_ms=", row.p50_ms, " p99_ms=", row.p99_ms, " worst_ms=", row.worst_ms, " phases=", row.phases, " movement=", moved)
	if OS.get_environment("SOE_PRF_35_BASELINE") != "1":
		_check(row.p50_ms < P50_MS, location + " magic p50 below 0.2 ms")
		_check(row.p99_ms < P99_MS, location + " magic p99 below 0.5 ms")
		_check(phases.has("translations") and phases.has("active_effects") and phases.has("concentration"), "phase report complete")
		system_overlay.panel.show()
		system_overlay.refresh()
		_check(system_overlay.label.text.contains("active_effects"), "F3 displays phases")
		await process_frame
		await RenderingServer.frame_post_draw
		var report := OS.get_environment("SOE_PRF_35_REPORT")
		if not report.is_empty():
			_check(root.get_texture().get_image().save_png(report.trim_suffix(".json") + "-" + location + ".png") == OK, "F3 capture written")


func _prf_35_light_hook_retains_vanilla_expiry() -> void:
	var previous_failures := failures
	var before: int = streamer.call("runtime_light_count")
	_check(player.call("cast_spell_on_self", 0, 0x00043324), "vanilla Candlelight cast")
	player.call("advance", DT)
	_check(streamer.call("runtime_light_count") == before + 1, "indexed summoned light creates RID")
	player.call("advance", DT)
	_check(streamer.call("runtime_light_count") == before + 1, "unchanged light keeps its RID")
	player.call("advance", 61.0)
	player.call("advance", DT)
	_check(streamer.call("runtime_light_count") == before, "Candlelight expiry releases RID")
	print("prf_35_light_hook_retains_vanilla_expiry: ", "PASS" if failures == previous_failures else "FAIL")
