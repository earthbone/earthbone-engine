extends "res://walkthrough.gd"

## The real free-walk path must keep time at low rendering rates.
const CLOCK_TOLERANCE := 0.05
const SAMPLE_SECONDS := 10.0
const MIN_GROUNDED_FRACTION := 0.8
var failures := 0
var results := {}


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("R4_RUNTIME FAIL: ", message)


func _sample(label: String, fps: int) -> void:
	Engine.max_fps = fps
	if player.has_method("set_detailed_tracing"):
		player.call("set_detailed_tracing", true)
	if label == "inn_20fps":
		player.call("set_facing", PI, 0.0)
	player.set("auto_step", true)
	await process_frame
	if label == "inn_20fps":
		var settle_deadline := Time.get_ticks_msec() + 5_000
		while not player.call("is_on_ground") and Time.get_ticks_msec() < settle_deadline:
			await process_frame
	var initial: Vector3 = player.call("position")
	var initial_ground: Dictionary = player.call("play_path_ground", initial)
	var before: Dictionary = player.call("budget_runtime_status")
	var start := Time.get_ticks_usec()
	var pipelines_before := Warmup.snapshot()
	var previous := start
	var samples: Array[Dictionary] = []
	Input.action_press("Forward")
	while Time.get_ticks_usec() - start < SAMPLE_SECONDS * 1e6:
		await process_frame
		var now := Time.get_ticks_usec()
		var runtime: Dictionary = player.call("budget_runtime_status")
		var systems: Dictionary = player.call("system_frame_breakdown")
		var feet: Vector3 = player.call("position")
		samples.append({
			"wall": (now - start) / 1e6,
			"game": float(runtime.seconds) - float(before.seconds),
			"frame_ms": (now - previous) / 1000.0,
			"streaming_ms": systems.cell_streaming.ms,
			"render_ms": systems.host_render_submission.ms,
			"physics_ms": systems.physics_sync.ms,
			"physics": player.call("physics_frame_breakdown"),
			"preparing": player.call("is_location_preparing"),
			"position": [feet.x, feet.y, feet.z],
			"grounded": player.call("is_on_ground"),
		})
		previous = now
	Input.action_release("Forward")
	var wall := (Time.get_ticks_usec() - start) / 1e6
	var after: Dictionary = player.call("budget_runtime_status")
	var game := float(after.seconds) - float(before.seconds)
	var finish: Vector3 = player.call("position")
	var pipeline_counts := Warmup.snapshot()
	for index in pipeline_counts.size():
		pipeline_counts[index] -= pipelines_before[index]
	var distance := Vector2(initial.x, initial.y).distance_to(Vector2(finish.x, finish.y))
	var grounded_samples := 0
	for sample in samples:
		if sample.grounded:
			grounded_samples += 1
	var grounded_fraction := float(grounded_samples) / samples.size()
	results[label] = {"fps_cap": fps, "wall": wall, "game": game, "ratio": game / wall, "distance": distance, "start": [initial.x, initial.y, initial.z], "finish": [finish.x, finish.y, finish.z], "initial_ground": initial_ground, "grounded": player.call("is_on_ground"), "grounded_fraction": grounded_fraction, "pipelines": Warmup.named(pipeline_counts), "frames": samples}
	print("R4_RUNTIME ", label, " wall=", wall, " game=", game, " ratio=", game / wall, " distance=", distance, " frames=", samples.size())
	print("R4_PIPELINES ", label, " ", JSON.stringify(results[label].pipelines))
	_check(absf(game / wall - 1.0) < CLOCK_TOLERANCE, label + " game time follows wall time within 5 percent")
	_check(distance > 100.0, label + " player moved at least 100 units")
	_check(not initial_ground.is_empty(), label + " starts above collision ground")
	if not initial_ground.is_empty():
		_check(absf(initial.z - float(initial_ground.height)) <= 4.0, label + " starts within four units of collision ground")
	_check(grounded_fraction >= MIN_GROUNDED_FRACTION, label + " stays grounded for at least 80 percent of frames")
	player.set("auto_step", false)
	if player.has_method("set_detailed_tracing"):
		player.call("set_detailed_tracing", false)


func _start() -> void:
	Engine.max_fps = 60
	await super._start()
	_check(loaded, "walkthrough loaded")
	if not loaded:
		quit(1)
		return
	await _sample("inn_20fps", 20)
	Engine.max_fps = 60
	player.call("set_detailed_tracing", true)
	var door_started := Time.get_ticks_usec()
	var door_previous := door_started
	var door_frames: Array[Dictionary] = []
	var crossed := false
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			crossed = player.call("activate_reference", door.file, door.object)
			break
	_check(crossed, "Bannered Mare exit activated")
	player.set("auto_step", true)
	var deadline := Time.get_ticks_msec() + 120_000
	while (player.call("budget_runtime_status").world_editor_id != "WhiterunWorld" or player.call("is_location_preparing")) and Time.get_ticks_msec() < deadline:
		await process_frame
		var now := Time.get_ticks_usec()
		var systems: Dictionary = player.call("system_frame_breakdown")
		door_frames.append({"wall": (now - door_started) / 1e6, "frame_ms": (now - door_previous) / 1000.0, "streaming_ms": systems.cell_streaming.ms, "preparing": player.call("is_location_preparing")})
		door_previous = now
	_check(not player.call("is_location_preparing"), "exterior ready")
	_check(player.call("budget_runtime_status").world_editor_id == "WhiterunWorld", "entered Whiterun")
	results["door_preparation"] = {"wall": (door_previous - door_started) / 1e6, "frames": door_frames}
	await _sample("door_walk_60fps", 60)
	var path := OS.get_environment("SOE_R4_REPORT")
	if not path.is_empty():
		var output := FileAccess.open(path, FileAccess.WRITE)
		_check(output != null, "report opened")
		if output != null:
			output.store_string(JSON.stringify(results, "\t"))
		_check(streamer.call("write_load_trace", path.get_basename() + ".trace.json"), "trace written")
	print("R4_RUNTIME ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
