extends "res://walkthrough.gd"

## Installed-data frame budget check for the walkthrough start and Whiterun exterior.
const STEP := 1.0 / 60.0
const FRAMES := 3600
const FRAME_P99_BUDGET_MS := 16.7
const FRAME_WORST_BUDGET_MS := 33.0

var failures := 0


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("TST-18 FAIL: ", message)


func _make_player(streamer: Node3D, cell: String, x: float, y: float, z: float) -> Node3D:
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "data source sharing failed for " + cell)
	_check(player.call("enter_cell", cell, x, y, z) > 0, "cell entry failed for " + cell)
	_check(player.call("start_gameplay"), "gameplay startup failed for " + cell)
	_check(player.call("prepare_ai_planning_check"), "navigation preparation failed for " + cell)
	return player


func _run_sixty_seconds(player: Node3D, label: String, minimum_actors: int) -> void:
	var actors: int = player.call("sync_contact_actors")
	_check(actors >= minimum_actors, "%s has only %d contact actors" % [label, actors])
	for _frame in 10:
		player.call("advance", STEP)
	var before: int = player.call("ai_package_evaluations")
	var frame_times: Array[float] = []
	for frame in FRAMES:
		var started := Time.get_ticks_usec()
		player.call("advance", STEP)
		var elapsed_ms := float(Time.get_ticks_usec() - started) / 1000.0
		frame_times.append(elapsed_ms)
		if elapsed_ms > FRAME_WORST_BUDGET_MS and frame_times.size() <= 3:
			printerr("TST-18 FAIL: %s frame %d took %.3f ms" % [label, frame, elapsed_ms])
	var evaluations: int = player.call("ai_package_evaluations") - before
	frame_times.sort()
	var p99_ms := frame_times[int(floor(float(FRAMES - 1) * 0.99))]
	var worst_ms := frame_times[FRAMES - 1]
	_check(evaluations >= actors, "%s ran no NPC package evaluations (%d actors, %d evaluations)" % [label, actors, evaluations])
	_check(p99_ms <= FRAME_P99_BUDGET_MS, "%s p99 %.3f ms exceeded %.1f ms" % [label, p99_ms, FRAME_P99_BUDGET_MS])
	_check(worst_ms <= FRAME_WORST_BUDGET_MS, "%s worst frame %.3f ms exceeded %.1f ms" % [label, worst_ms, FRAME_WORST_BUDGET_MS])
	print("TST-18 %s actors=%d frames=%d simulated_seconds=60 evaluations=%d p99_ms=%.3f worst_ms=%.3f" % [label, actors, FRAMES, evaluations, p99_ms, worst_ms])


func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("TST-18 requires SKYRIM_DATA and the built Godot extension")
		quit(1)
		return
	await super._start()
	_check(loaded and is_instance_valid(player), "walkthrough startup failed")
	if not loaded or not is_instance_valid(player):
		quit(1)
		return
	var interior := player
	var interior_scene := interior.get_parent()
	var interior_streamer: Node3D = interior_scene.get_child(0)
	interior.set("auto_step", false)
	await _run_sixty_seconds(interior, "Bannered Mare walkthrough start", 5)
	interior.free()
	system_overlay.free()
	var exterior_cells: Array = interior_streamer.call("editor_cell_tree")
	var exterior_name := ""
	for cell: Dictionary in exterior_cells:
		if not cell.interior and cell.world_name == "WhiterunWorld" and cell.name == "WhiterunOrigin":
			exterior_name = cell.name
			break
	_check(not exterior_name.is_empty(), "WhiterunOrigin exterior cell is absent")
	if not exterior_name.is_empty():
		_check(interior_streamer.call("stream_exterior", "WhiterunOrigin", 1) >= 0, "Whiterun exterior stream failed")
		var exterior := _make_player(interior_streamer, exterior_name, 2048.0, 2048.0, 500.0)
		await _run_sixty_seconds(exterior, "WhiterunWorld exterior", 1)
		exterior.free()
	interior_scene.free()
	print("TST-18 result failures=", failures)
	quit(0 if failures == 0 else 1)
