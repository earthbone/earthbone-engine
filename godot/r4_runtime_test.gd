extends "res://walkthrough.gd"

## The real free-walk path must keep time at low rendering rates.
const CLOCK_TOLERANCE := 0.05
const SAMPLE_SECONDS := 10.0
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
	if label == "inn_20fps":
		player.call("set_facing", PI, 0.0)
	player.set("auto_step", true)
	await process_frame
	var initial: Vector3 = player.call("position")
	var before: Dictionary = player.call("budget_runtime_status")
	var start := Time.get_ticks_usec()
	var previous := start
	var samples: Array[Dictionary] = []
	Input.action_press("Forward")
	while Time.get_ticks_usec() - start < SAMPLE_SECONDS * 1e6:
		await process_frame
		var now := Time.get_ticks_usec()
		var runtime: Dictionary = player.call("budget_runtime_status")
		var systems: Dictionary = player.call("system_frame_breakdown")
		samples.append({
			"wall": (now - start) / 1e6,
			"game": float(runtime.seconds) - float(before.seconds),
			"frame_ms": (now - previous) / 1000.0,
			"streaming_ms": systems.cell_streaming.ms,
			"render_ms": systems.host_render_submission.ms,
			"physics_ms": systems.physics_sync.ms,
			"physics": player.call("physics_frame_breakdown"),
			"preparing": player.call("is_location_preparing"),
		})
		previous = now
	Input.action_release("Forward")
	var wall := (Time.get_ticks_usec() - start) / 1e6
	var after: Dictionary = player.call("budget_runtime_status")
	var game := float(after.seconds) - float(before.seconds)
	var finish: Vector3 = player.call("position")
	var distance := Vector2(initial.x, initial.y).distance_to(Vector2(finish.x, finish.y))
	results[label] = {"fps_cap": fps, "wall": wall, "game": game, "ratio": game / wall, "distance": distance, "frames": samples}
	print("R4_RUNTIME ", label, " wall=", wall, " game=", game, " ratio=", game / wall, " distance=", distance, " frames=", samples.size())
	_check(absf(game / wall - 1.0) < CLOCK_TOLERANCE, label + " game time follows wall time within 5 percent")
	_check(distance > 100.0, label + " player moved at least 100 units")
	player.set("auto_step", false)


func _start() -> void:
	Engine.max_fps = 60
	await super._start()
	_check(loaded, "walkthrough loaded")
	if not loaded:
		quit(1)
		return
	await _sample("inn_20fps", 20)
	Engine.max_fps = 60
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
	_check(not player.call("is_location_preparing"), "exterior ready")
	_check(player.call("budget_runtime_status").world_editor_id == "WhiterunWorld", "entered Whiterun")
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
