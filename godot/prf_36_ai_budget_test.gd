extends "res://walkthrough.gd"

## PRF-36 windowed walkthrough AI costs, including caller worker waits.
const STEP := 1.0 / 60.0
const P50_MS := 0.6
const P99_MS := 1.2
var failures := 0

func _initialize() -> void:
	call_deferred("prf_36_installed_walkthrough_ai_budget")

func _configure_player() -> void:
	super._configure_player()
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-36 FAIL: ", message)

func _sample(label: String, walking: bool) -> void:
	var samples: Array[float] = []
	var planning_worst := 0.0
	var before: int = player.call("ai_package_evaluations")
	var runtime_before: Dictionary = player.call("budget_runtime_status")
	var worker_jobs := 0
	var worker_busy_us := 0.0
	var start: Vector3 = player.call("position")
	var moved := 0.0
	var baseline := OS.get_environment("SOE_PRF_36_BASELINE") == "1"
	var count := 120 if baseline else 3600
	for frame in count:
		if frame % 60 == 0:
			await process_frame
		if walking:
			var facing: Vector2 = player.call("facing")
			var sign := 1.0 if int(frame / 300.0) % 2 == 0 else -1.0
			player.call("sample_detection_walk", STEP, Vector3(sin(facing.x), cos(facing.x), 0) * sign)
		_check(player.call("begin_budget_frame"), "open timing frame")
		player.call("advance", STEP)
		var report: Dictionary = player.call("end_budget_frame")
		samples.append(report.systems.ai_movement + report.systems.ai_planning)
		var phases: Dictionary = player.call("ai_planning_details")
		worker_jobs += int(phases.get("movement_worker_jobs", 0)) + int(phases.get("navigation_worker_jobs", 0))
		worker_busy_us += float(phases.get("movement_worker_busy_us", 0.0)) + float(phases.get("navigation_worker_busy_us", 0.0))
		planning_worst = maxf(planning_worst, float(player.call("ai_planning_microseconds")) / 1000.0)
		moved = maxf(moved, Vector2(start.x, start.y).distance_to(Vector2(player.call("position").x, player.call("position").y)))
		if frame % 600 == 0:
			print("PRF-36 ", label, " frame=", frame, " phases=", phases)
	samples.sort()
	var p50 := samples[int(ceil(count * 0.5)) - 1]
	var p99 := samples[int(ceil(count * 0.99)) - 1]
	var evaluations: int = player.call("ai_package_evaluations") - before
	var runtime_after: Dictionary = player.call("budget_runtime_status")
	_check(absf(float(runtime_after.seconds) - float(runtime_before.seconds) - STEP * count) < 0.001, label + " ran every simulation step")
	print("PRF-36 ", label, " frames=", count, " p50_ms=", p50, " p99_ms=", p99, " driver_worst_ms=", planning_worst, " evaluations=", evaluations, " walked_units=", moved, " worker_jobs=", worker_jobs, " worker_busy_mean_ms=", worker_busy_us / count / 1000.0)
	_check(evaluations > 0, label + " evaluated NPC packages")
	if not baseline:
		_check(worker_jobs > 0, label + " exercised workers")
		_check(planning_worst < 8, label + " driver under 8 ms")
		if walking:
			_check(p50 < P50_MS, label + " combined p50 under 0.6 ms")
			_check(p99 < P99_MS, label + " combined p99 under 1.2 ms")
			_check(moved > 100, label + " player walked")

func prf_36_installed_walkthrough_ai_budget() -> void:
	if OS.get_environment("SKYRIM_DATA").is_empty():
		print("SKIP PRF-36: SKYRIM_DATA is unset")
		quit(0)
		return
	await super._start()
	_check(loaded, "walkthrough startup")
	if not loaded:
		quit(1)
		return
	_check(player.call("prepare_ai_planning_check"), "navigation ready")
	await _sample("Bannered Mare", false)
	var crossed := false
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			crossed = player.call("activate_reference", door.file, door.object)
			break
	_check(crossed, "interior exit")
	player.call("advance", 0.0)
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "Whiterun gate")
	if not gate.is_empty():
		_check(player.call("queue_player_input", {"action": "move_to", "file": gate.file, "object": gate.object, "offset": Vector3(0, 200, 0), "match_rotation": true}), "gate placement")
		_check(player.call("step_player_input"), "apply gate placement")
		_check(player.call("activate_reference", gate.file, gate.object), "gate exit")
		_check(player.call("detection_worldspace") == "Tamriel", "plains worldspace")
		player.call("advance", 0.0)
		_check(player.call("sync_contact_actors") > 1, "plains NPC bodies")
		await _sample("Whiterun plains", true)
		if OS.get_environment("SOE_PRF_36_BASELINE") != "1":
			await prf_36_live_plains_presentation_budget()
	player.free()
	player = null
	system_overlay.free()
	streamer.get_parent().free()
	print("PRF-36 failures=", failures)
	quit(0 if failures == 0 else 1)

func _key(code: int, pressed: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = code
	event.physical_keycode = code
	event.pressed = pressed
	Input.parse_input_event(event)
	Input.flush_buffered_events()

func prf_36_live_plains_presentation_budget() -> void:
	player.set("auto_step", true)
	for _frame in 10:
		await process_frame
	var start := Time.get_ticks_usec()
	var samples: Array[float] = []
	var before: Dictionary = player.call("budget_runtime_status")
	var initial: Vector3 = player.call("position")
	var moved := 0.0
	var detection_samples := 0
	var previous := -1
	while Time.get_ticks_usec() - start < 60000000:
		var phase := int((Time.get_ticks_usec() - start) / 5000000.0) % 2
		if phase != previous:
			_key(KEY_W, phase == 0)
			_key(KEY_S, phase == 1)
			previous = phase
		await process_frame
		var report: Dictionary = player.call("system_frame_breakdown")
		samples.append(float(report.ai_movement.ms) + float(report.ai_planning.ms))
		if int(player.call("detection_time_usec")) >= 0:
			detection_samples += 1
		var current: Vector3 = player.call("position")
		moved = maxf(moved, Vector2(current.x, current.y).distance_to(Vector2(initial.x, initial.y)))
	_key(KEY_W, false)
	_key(KEY_S, false)
	player.set("auto_step", false)
	var after: Dictionary = player.call("budget_runtime_status")
	samples.sort()
	_check(not samples.is_empty(), "live presentation frames")
	if samples.is_empty():
		return
	var p50 := samples[int(ceil(samples.size() * 0.5)) - 1]
	var p99 := samples[int(ceil(samples.size() * 0.99)) - 1]
	var ticks := int(after.ticks) - int(before.ticks)
	print("PRF-36 live plains frames=", samples.size(), " p50_ms=", p50, " p99_ms=", p99, " simulation_seconds=", float(after.seconds) - float(before.seconds), " ticks_per_frame=", float(ticks) / samples.size(), " walked_units=", moved, " detection_samples=", detection_samples)
	_check(p50 < P50_MS, "live plains combined p50 under 0.6 ms")
	_check(p99 < P99_MS, "live plains combined p99 under 1.2 ms")
	_check(int(after.evaluations) > int(before.evaluations), "live NPC evaluations")
	_check(detection_samples > 0, "live detection samples completed")
	var tick_seconds := float(int(after.ticks) - int(before.ticks)) * STEP
	_check(tick_seconds > 1.0 and absf(float(after.seconds) - float(before.seconds) - tick_seconds) < 0.1, "live simulation was not menu paused")
	_check(moved > 100.0, "live player walked")
