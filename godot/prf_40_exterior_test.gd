extends "res://walkthrough.gd"

## Fixed exterior traversal on the default gameplay path, including streaming.
## Godot pipeline counters describe compile counts, not compiler durations.
const SAMPLES := 360
var results := {}
var failures := 0


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-40 FAIL: ", message)


func _start() -> void:
	_check(DisplayServer.get_name() != "headless", "windowed renderer required")
	Engine.max_fps = 60
	await super._start()
	if not loaded:
		quit(1)
		return
	var target := OS.get_environment("SOE_PRF40_CELL")
	if target.is_empty():
		target = "WhiterunExterior04"
	var exterior := {}
	for cell: Dictionary in streamer.call("editor_cell_tree"):
		if cell.name == target and not cell.interior:
			exterior = cell
			break
	_check(not exterior.is_empty(), "target exterior CELL exists")
	if exterior.is_empty():
		quit(1)
		return
	startup_cell = target
	player.set("auto_step", false)
	var warmed_variants := 0
	if streamer.has_method("upload_progress"):
		warmed_variants = streamer.call("upload_progress").shader_variants
		_check(warmed_variants > 0, "startup created custom shader variants")
	streamer.call("clear")
	if streamer.has_method("upload_progress"):
		_check(streamer.call("upload_progress").shader_variants == warmed_variants, "cell retirement retained warmed shader resources")
	_check(streamer.call("stream_exterior", target, 1) > 0, "initial exterior rendering")
	var point := Vector3(exterior.x * 4096.0 + 4000.0, exterior.y * 4096.0 + 2048.0, 5000.0)
	var floor_height: Variant = streamer.call("play_path_terrain_height", Vector3(point.x / 70.0, 0, -point.y / 70.0))
	if floor_height != null:
		point.z = float(floor_height) + 160.0
	_check(player.call("enter_cell", target, point.x, point.y, point.z) >= 0, "exterior controller entry")
	player.set("auto_step", true)
	await process_frame
	await process_frame
	var deadline := Time.get_ticks_msec() + 120_000
	while player.call("is_location_preparing") and Time.get_ticks_msec() < deadline:
		await process_frame
	_check(not player.call("is_location_preparing"), "initial location preparation completed")
	player.set("auto_step", false)
	var support: Dictionary = player.call("play_path_ground", point + Vector3(0, 0, 4096))
	_check(not support.is_empty(), "starting point has collision support")
	if not support.is_empty():
		point.z = float(support.height) + 32.0
		_check(player.call("enter_cell", target, point.x, point.y, point.z) >= 0, "start on collision surface")
	player.call("advance", 0.0)
	player.set("auto_step", true)
	deadline = Time.get_ticks_msec() + 120_000
	await process_frame
	while player.call("is_location_preparing") and Time.get_ticks_msec() < deadline:
		await process_frame
	_check(not player.call("is_location_preparing"), "grounded location preparation completed")
	var before := Warmup.snapshot()
	var upload_before := {}
	if streamer.has_method("upload_progress"):
		upload_before = streamer.call("upload_progress")
	var frames_ms: Array[float] = []
	var streaming_ms: Array[float] = []
	var render_ms: Array[float] = []
	var physics_ms: Array[float] = []
	var max_steps := 0
	var start: Vector3 = player.call("position")
	var initial_ground: Dictionary = player.call("play_path_ground", start + Vector3(0, 0, 128))
	player.call("set_detailed_tracing", true)
	var start_cell: String = player.call("gameplay_cell")
	var residents_before: Dictionary = player.call("exterior_resources")
	var previous := Time.get_ticks_usec()
	player.call("set_facing", PI / 2.0, 0.0)
	Input.action_press("Forward")
	for index in SAMPLES:
		await process_frame
		var now := Time.get_ticks_usec()
		frames_ms.append((now - previous) / 1000.0)
		previous = now
		var systems: Dictionary = player.call("system_frame_breakdown")
		streaming_ms.append(systems.cell_streaming.ms)
		render_ms.append(systems.host_render_submission.ms)
		physics_ms.append(systems.physics_sync.ms)
		var physics: Dictionary = player.call("physics_frame_breakdown")
		max_steps = maxi(max_steps, physics.steps)
		if index == 179:
			Input.action_release("Forward")
	Input.action_release("Forward")
	player.call("set_detailed_tracing", false)
	var after := Warmup.snapshot()
	var delta: Array[int] = []
	for index in before.size():
		delta.append(after[index] - before[index])
	var finish: Vector3 = player.call("position")
	results = {
		"cell": startup_cell,
		"warmed_shader_variants": warmed_variants,
		"display": DisplayServer.get_name(),
		"renderer": RenderingServer.get_video_adapter_name(),
		"frames": frames_ms,
		"streaming_ms": streaming_ms,
		"render_ms": render_ms,
		"physics_ms": physics_ms,
		"pipelines": Warmup.named(delta),
		"max_physics_steps": max_steps,
		"distance": start.distance_to(finish),
		"horizontal_distance": Vector2(start.x, start.y).distance_to(Vector2(finish.x, finish.y)),
		"start": [start.x, start.y, start.z],
		"initial_ground": initial_ground,
		"finish": [finish.x, finish.y, finish.z],
		"on_ground": player.call("is_on_ground"),
		"start_cell": start_cell,
		"end_cell": player.call("gameplay_cell"),
		"residents_before": residents_before.size(),
		"residents_after": player.call("exterior_resources").size(),
	}
	if streamer.has_method("upload_progress"):
		var upload_after: Dictionary = streamer.call("upload_progress")
		results["upload"] = upload_after
		results["upload_slices_measured"] = upload_after.slices - upload_before.slices
	for field in ["frames", "streaming_ms", "render_ms", "physics_ms"]:
		var values: Array = results[field].duplicate()
		values.sort()
		results[field + "_p50"] = values[ceili(values.size() * 0.5) - 1]
		results[field + "_p99"] = values[ceili(values.size() * 0.99) - 1]
		results[field + "_max"] = values.back()
	_check(max_steps <= 1, "physics catch-up exceeded one step")
	_check(results.streaming_ms_max < 33.0, "streaming stays below 33 ms")
	_check(results.distance > 128.0, "player moved through the exterior")
	_check(results.start_cell != results.end_cell, "traversal crossed a CELL boundary")
	_check(results.on_ground, "traversal finished on collision ground")
	var path := OS.get_environment("SOE_PRF40_REPORT")
	if not path.is_empty():
		var output := FileAccess.open(path, FileAccess.WRITE)
		_check(output != null, "report opened")
		if output != null:
			output.store_string(JSON.stringify(results, "\t"))
		_check(streamer.call("write_load_trace", path.get_basename() + ".trace.json"), "load trace exported")
	var brief := results.duplicate()
	for field in ["frames", "streaming_ms", "render_ms", "physics_ms"]:
		brief.erase(field)
	print("PRF-40 ", JSON.stringify(brief))
	print("prf_40_installed_exterior: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
