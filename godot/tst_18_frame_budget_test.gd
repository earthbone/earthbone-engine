extends "res://walkthrough.gd"

## Completed PRF-12 render frames. Simulation and worker times stay separate.
const Budget := preload("res://simulation_budget.gd")
const SAMPLES := 240
var failures := 0
var results := {}


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", true)


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("TST-18 FAIL: ", message)


func _ready_simulation() -> bool:
	var deadline := Time.get_ticks_msec() + 90_000
	while Time.get_ticks_msec() < deadline:
		await process_frame
		var status: Dictionary = player.call("simulation_status")
		if not status.error.is_empty():
			_check(false, status.error)
			return false
		if status.ready:
			return true
	_check(false, "simulation never published a first snapshot")
	return false


func _measure(location: String) -> void:
	if not await _ready_simulation():
		return
	var warmup := Time.get_ticks_msec() + 2000
	while Time.get_ticks_msec() < warmup:
		await process_frame
	var before: Dictionary = player.call("simulation_status")
	var main: Array[float] = []
	var simulation: Array[float] = []
	var workers: Array[float] = []
	var lag := 0.0
	var systems := {}
	for index in SAMPLES:
		await process_frame
		var report: Dictionary = player.call("system_frame_breakdown")
		var status: Dictionary = player.call("simulation_status")
		_check(status.error.is_empty(), "worker failed in " + location)
		main.append(report.main_thread_engine.ms)
		for system: String in report:
			if system != "main_thread_engine":
				systems[system] = systems.get(system, 0.0) + report[system].ms
		simulation.append(status.tick_ms)
		workers.append(status.worker_ms)
		lag = maxf(lag, status.behind_seconds)
		if index % 80 == 79:
			print("TST-18 progress ", location, " ", index + 1)
	var after: Dictionary = player.call("simulation_status")
	var row := Budget.evaluate(main, lag, after.ticks - before.ticks)
	row["raw_main_ms"] = main
	row["raw_simulation_ms"] = simulation
	row["raw_worker_ms"] = workers
	for system: String in systems:
		systems[system] /= SAMPLES
	row["mean_system_ms"] = systems
	results[location] = row
	_check(row.passed, location + " main thread 2 ms p50 / 4 ms p99 / 1 s lag gate")
	_check(after.actors > 1 and after.evaluations >= after.actors, location + " has no running NPC packages")
	row["actors"] = after.actors
	row["evaluations"] = after.evaluations
	print("TST-18 ", location, " p50_ms=", row.p50_ms, " p99_ms=", row.p99_ms, " behind_seconds=", lag, " ticks=", row.ticks, " actors=", after.actors)
	system_overlay.panel.show()
	system_overlay.refresh()
	await process_frame
	await RenderingServer.frame_post_draw
	var path := OS.get_environment("SOE_PRF_37_REPORT")
	if not path.is_empty():
		_check(root.get_texture().get_image().save_png(path.trim_suffix(".json") + "-" + location + ".png") == OK, "F3 capture saved")


func _start() -> void:
	if OS.get_environment("SKYRIM_DATA").is_empty():
		print("prf_37_installed_windowed_tst_18_gate: SKIP SKYRIM_DATA unset")
		quit(0)
		return
	_check(DisplayServer.get_name() != "headless", "windowed rendering required")
	await super._start()
	if not loaded:
		quit(1)
		return
	await _measure("bannered_mare")
	player.call("stop_threaded_gameplay")
	player.set("auto_step", false)
	var plains := {}
	for cell: Dictionary in streamer.call("editor_cell_tree"):
		if not cell.interior and cell.world_name == "Tamriel" and cell.name == "WhiterunExterior04":
			plains = cell
			break
	_check(not plains.is_empty(), "Whiterun plains cell exists")
	if not plains.is_empty():
		streamer.call("clear")
		_check(streamer.call("stream_exterior", plains.name, 1) >= 0, "plains render stream")
		_check(player.call("enter_cell", plains.name, plains.x * 4096.0 + 2048.0, plains.y * 4096.0 + 2048.0, 5000.0) > 0, "plains controller entry")
		_check(player.call("start_threaded_gameplay"), "plains simulation startup")
		player.set("auto_step", true)
		await _measure("whiterun_plains")
	var path := OS.get_environment("SOE_PRF_37_REPORT")
	if not path.is_empty():
		var report := FileAccess.open(path, FileAccess.WRITE)
		_check(report != null, "timing report opened")
		if report != null:
			report.store_string(JSON.stringify(results, "\t"))
	print("prf_37_installed_windowed_tst_18_gate: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
