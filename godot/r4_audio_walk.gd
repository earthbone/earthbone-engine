extends "res://walkthrough.gd"

const AUDIO_LIMIT_MS := 2.0
var checking := false
var worst_audio_ms := 0.0
var failures: Array[String] = []
var graph_steps := 0
var decode_ns := 0


func _initialize() -> void:
	var cell := OS.get_environment("R4_AUDIO_CELL")
	if not cell.is_empty():
		startup_cell = cell
	startup_position = Vector3(-125.0, -800.0, 120.0)
	if _is_exterior():
		startup_radius = 1
		startup_position = Vector3(2048.0, 2048.0, 500.0)
	super._initialize()


func _configure_player() -> void:
	player.set("capture_mouse", false)


func _process(delta: float) -> bool:
	if not loaded:
		return super._process(delta)
	var report: Dictionary = player.call("system_frame_breakdown")
	worst_audio_ms = maxf(worst_audio_ms, float(report.audio.ms))
	var metrics: Dictionary = player.call("performance_metrics")
	var decoded := int(metrics.audio_decode_ns)
	if float(report.audio.ms) > AUDIO_LIMIT_MS:
		print("R4 AUDIO_HITCH audio_ms=", report.audio.ms,
			" decoded_ms=", (decoded - decode_ns) / 1_000_000.0)
	decode_ns = decoded
	if not checking:
		checking = true
		_exercise()
	return false


func _key(code: Key, pressed: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = code
	event.pressed = pressed
	Input.parse_input_event(event)
	Input.flush_buffered_events()


func _graph_event(tag: String) -> void:
	if tag.begins_with("Foot"):
		graph_steps += 1


func _is_exterior() -> bool:
	return startup_cell.begins_with("WhiterunOrigin") or startup_cell.begins_with("WhiterunExterior")


func _stand_on_exterior_ground() -> void:
	## The exterior spawn height is a guess. Free fall through missing
	## collision made the walk record zero steps, so settle on measured ground.
	var exterior := {}
	for cell: Dictionary in streamer.call("editor_cell_tree"):
		if cell.name == startup_cell and not cell.interior:
			exterior = cell
			break
	if exterior.is_empty():
		failures.append("exterior cell is not in the cell tree")
		return
	var point := Vector3(exterior.x * 4096.0 + 2048.0, exterior.y * 4096.0 + 2048.0, 5000.0)
	var floor_height: Variant = streamer.call("play_path_terrain_height", Vector3(point.x / 70.0, 0, -point.y / 70.0))
	if floor_height != null:
		point.z = float(floor_height) + 160.0
	player.call("enter_cell", startup_cell, point.x, point.y, point.z)
	await process_frame
	await process_frame
	var deadline := Time.get_ticks_msec() + 120_000
	while player.call("is_location_preparing") and Time.get_ticks_msec() < deadline:
		await process_frame
	var support: Dictionary = player.call("play_path_ground", point + Vector3(0, 0, 4096))
	if not support.is_empty():
		point.z = float(support.height) + 32.0
		player.call("enter_cell", startup_cell, point.x, point.y, point.z)
		await process_frame
	deadline = Time.get_ticks_msec() + 20_000
	while not player.call("is_on_ground") and Time.get_ticks_msec() < deadline:
		await process_frame
	if not player.call("is_on_ground"):
		failures.append("player never reached collision ground at " + str(player.call("position")))


func _exercise() -> void:
	if _is_exterior():
		await _stand_on_exterior_ground()
	await create_timer(2.0).timeout
	print("R4 START position=", player.call("position"), " on_ground=", player.call("is_on_ground"))
	var metrics_before: Dictionary = player.call("performance_metrics")
	var trace_path := OS.get_environment("R4_TRACE_PATH")
	if trace_path != "":
		player.call("set_detailed_tracing", true)
	if player_body == null:
		failures.append("free walk has no player behavior graph")
	else:
		player_body.connect("animation_event", _graph_event)
	for perspective in range(2):
		var before := int(player.call("footstep_count"))
		for direction in [KEY_W, KEY_S, KEY_W, KEY_S]:
			_key(direction, true)
			await create_timer(1.5).timeout
			_key(direction, false)
		var played := int(player.call("footstep_count")) - before
		print("R4 AFTER position=", player.call("position"), " on_ground=", player.call("is_on_ground"))
		print("R4 WALK cell=", startup_cell, " perspective=", perspective, " steps=", played)
		if played < 2:
			failures.append("fewer than two player steps in perspective " + str(perspective))
		var previous_camera: bool = player.call("is_third_person")
		_key(KEY_F, true)
		await process_frame
		_key(KEY_F, false)
		await process_frame
		if bool(player.call("is_third_person")) == previous_camera:
			failures.append("camera mode did not change")
	if trace_path != "":
		## Extra free walking so a trace covers about a minute of NPC updates.
		var extra_end := Time.get_ticks_msec() + int(OS.get_environment("R4_TRACE_SECONDS").to_int()) * 1000
		while Time.get_ticks_msec() < extra_end:
			for direction in [KEY_W, KEY_S]:
				_key(direction, true)
				await create_timer(2.0).timeout
				_key(direction, false)
		streamer.call("write_load_trace", trace_path)
	await create_timer(1.0).timeout
	var stopped := int(player.call("footstep_count"))
	await create_timer(2.0).timeout
	if int(player.call("footstep_count")) != stopped:
		failures.append("standing player kept playing locomotion footsteps")
	if graph_steps < 4:
		failures.append("behavior graph emitted fewer than four foot tags")
	if int(player.call("footstep_errors")) != 0:
		failures.append("footstep playback reported errors")
	var selections: Array = player.call("footstep_selections")
	if selections.is_empty():
		failures.append("no descriptor selections were recorded")
	for selection: Dictionary in selections:
		print("R4 SELECTION ", JSON.stringify(selection))
	print("R4 AUDIO worst_ms=", worst_audio_ms, " budget_ms=", AUDIO_LIMIT_MS,
		" decode_ms=", (int(player.call("performance_metrics").audio_decode_ns) - int(metrics_before.audio_decode_ns)) / 1_000_000.0,
		" result=", "PASS" if worst_audio_ms <= AUDIO_LIMIT_MS else "FAIL")
	print("R4 FOOTSTEPS cell=", startup_cell, " graph_steps=", graph_steps,
		" result=", "PASS" if failures.is_empty() else "FAIL", " failures=", failures)
	quit(0 if failures.is_empty() else 1)
