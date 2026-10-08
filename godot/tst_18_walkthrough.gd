extends "res://walkthrough.gd"

## TST-18 headless gameplay frames, following the walkthrough and local
## Godot Time docs. Budgets are fixed in docs/performance.md before runs.
const Metrics := preload("res://bench_metrics.gd")
const SECONDS := 60.0
const P99_MS := 16.7
const WORST_MS := 33.0
const SETTLE_USEC := 20_000_000
const NAVIGATION_INTERVAL := 60

var samples: Array[float] = []
var started_usec := 0
var previous_usec := 0
var evaluations_before := 0
var violations := 0
var worst_frame := 0
var worst_ms := 0.0
var selected_frames := 0
var finished := false
var ready_usec := 0
var expected_cell := ""
var cell_violations := 0
var navigation_frames := 0
var navigation_queries := 0


func _initialize() -> void:
	Engine.max_fps = 0
	OS.low_processor_usage_mode = false
	if OS.get_cmdline_user_args() == PackedStringArray(["WhiterunOrigin"]):
		startup_cell = "WhiterunOrigin"
		startup_radius = 1
		startup_position = Vector3(2048.0, 2048.0, 500.0)
	_watchdog()
	super._initialize()

func _spawn_position(streamer: Node3D) -> Vector3:
	if startup_radius < 0:
		return startup_position
	var stats: Dictionary = streamer.call("last_stats")
	return Vector3((int(stats.origin_x) + 0.5) * 4096.0, (int(stats.origin_y) + 0.5) * 4096.0, startup_position.z)


func _watchdog() -> void:
	await create_timer(210.0).timeout
	if not finished:
		finished = true
		printerr("TST-18 FAIL: startup or measurement exceeded 210 seconds")
		quit(1)


func _start() -> void:
	await super._start()
	if loaded:
		player.set("auto_step", false)
		ready_usec = Time.get_ticks_usec()
		expected_cell = player.call("gameplay_cell")
	else:
		finished = true


func _process(_delta: float) -> bool:
	if finished:
		return true
	if not loaded:
		return false
	var now := Time.get_ticks_usec()
	if now - ready_usec < SETTLE_USEC:
		player.call("advance", 1.0 / 60.0)
		return false
	if started_usec == 0:
		started_usec = now
		previous_usec = now
		evaluations_before = player.call("ai_package_evaluations")
	var elapsed_ms := (now - previous_usec) / 1000.0
	previous_usec = now
	# CONTEXT: The next interval includes this advance and all host callbacks.
	if not samples.is_empty():
		if not is_finite(elapsed_ms) or elapsed_ms <= 0.0:
			printerr("TST-18 FAIL: invalid frame interval")
			quit(1)
			return true
		if elapsed_ms > worst_ms:
			worst_ms = elapsed_ms
			worst_frame = samples.size()
		if elapsed_ms > WORST_MS:
			violations += 1
			if violations <= 3:
				printerr("TST-18 FAIL: %s frame %d %.3f ms > %.3f ms" % [startup_cell, samples.size(), elapsed_ms, WORST_MS])
	# CONTEXT: A sentinel starts the interval before the first advance.
	samples.append(elapsed_ms)
	if now - started_usec >= int(SECONDS * 1_000_000) and (samples.size() > 3600 or violations > 0):
		_finish(now)
		return true
	player.call("advance", 1.0 / 60.0)
	# CONTEXT: Match AI's one-second package evaluation interval.
	if (samples.size() - 1) % NAVIGATION_INTERVAL == 0:
		var navigation: Dictionary = player.call("probe_npc_navigation")
		navigation_queries += int(navigation.queries)
		if navigation.queries > 0 and navigation.located > 0:
			navigation_frames += 1
	if player.call("gameplay_cell") != expected_cell:
		cell_violations += 1
	var details: Dictionary = player.call("ai_planning_details")
	if details.get("selected_packages", 0) > 0:
		selected_frames += 1
	return false


func _finish(now: int) -> void:
	finished = true
	samples.remove_at(0)
	var details: Dictionary = player.call("ai_planning_details")
	var evaluations: int = player.call("ai_package_evaluations") - evaluations_before
	var row := {
		"cell": startup_cell,
		"headless": DisplayServer.get_name() == "headless",
		"samples": samples.size(),
		"elapsed_s": (now - started_usec) / 1_000_000.0,
		"simulated_s": samples.size() / 60.0,
		"settle_s": SETTLE_USEC / 1_000_000.0,
		"cell_violations": cell_violations,
		"navigation_frames": navigation_frames,
		"navigation_expected_frames": int(ceil(samples.size() / float(NAVIGATION_INTERVAL))),
		"navigation_queries": navigation_queries,
		"p99_ms": Metrics.percentile(samples, 0.99),
		"worst_ms": worst_ms,
		"worst_frame": worst_frame,
		"over_budget_frames": violations,
		"selected_frames": selected_frames,
		"actors": details.get("instantiated_actors", 0),
		"selected_packages": details.get("selected_packages", 0),
		"navmesh_triangles": details.get("navmesh_triangles", 0),
		"evaluations": evaluations,
	}
	print("TST_18_SAMPLE ", JSON.stringify(row))
	var active: bool = row.actors >= 5 and row.selected_packages > 0 and row.navmesh_triangles > 0 and evaluations >= row.actors and selected_frames == samples.size() and navigation_frames == row.navigation_expected_frames and cell_violations == 0
	var passed: bool = active and violations == 0 and row.p99_ms <= P99_MS and row.worst_ms <= WORST_MS
	if not active:
		printerr("TST-18 FAIL: NPC packages or navigation were inactive")
	# CONTEXT: The per-frame print above stops at three, so the totals go here.
	if not passed:
		printerr("TST-18 FAIL: %s over_budget_frames=%d p99_ms=%.3f (limit %.1f) worst_ms=%.3f (limit %.1f) samples=%d" % [startup_cell, violations, row.p99_ms, P99_MS, row.worst_ms, WORST_MS, samples.size()])
	print("RESULT: PASS" if passed else "RESULT: FAIL")
	quit(0 if passed else 2)
