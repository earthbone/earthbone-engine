extends SceneTree

## Operation counts exercise the live RID path without measuring rendering speed.

const NAMES := ["navmesh_locate", "navmesh_triangles", "path_nodes", "papyrus_instructions", "native_calls", "conditions", "raycasts", "shape_casts", "record_lookups", "render_instances"]
var failures := 0


func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr(message)


func _initialize() -> void:
	call_deferred("run")


func run() -> void:
	var scene := Node3D.new()
	root.add_child(scene)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	if streamer == null:
		printerr("CellStreamerNode is unavailable")
		quit(1)
		return
	scene.add_child(streamer)
	streamer.set("cell_occluders", false)
	print("PRF_14_PHASE load_data")
	check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "load_data failed")
	if failures > 0:
		quit(1)
		return
	print("PRF_14_PHASE stream_cell")
	streamer.call("begin_work_frame")
	var built: int = streamer.call("stream_cell", "WhiterunBanneredMare")
	var counts: Dictionary = streamer.call("end_work_frame")
	check(built > 0, "cell has no instances")
	for name in NAMES:
		check(counts.has(name), "missing counter " + name)
	check(counts.get("render_instances", 0) == built, "RID creations differ from streamed instances")
	check(counts.get("record_lookups", 0) > 0, "streaming did not count record lookups")
	check(counts.get("render_instances", 0) <= 2048, "cell exceeds instance budget")
	check(counts.get("record_lookups", 0) <= 100000, "cell exceeds lookup budget")
	streamer.call("begin_work_frame")
	var empty: Dictionary = streamer.call("end_work_frame")
	for name in NAMES:
		check(empty.get(name, -1) == 0, "new frame retains " + name)
	print("PRF_14_PHASE player_collision")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	if player == null:
		printerr("PlayerNode is unavailable")
		quit(1)
		return
	player.set("auto_step", false)
	player.set("capture_mouse", false)
	scene.add_child(player)
	check(player.call("use_streamer_data", streamer), "player data binding failed")
	check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "player collision failed")
	print("PRF_14_PHASE start_gameplay")
	check(player.call("start_gameplay"), "gameplay startup failed")
	print("PRF_14_PHASE advance")
	player.call("advance", 1.0 / 60.0)
	var player_counts: Dictionary = player.call("work_counts")
	for name in NAMES:
		check(player_counts.has(name), "missing player counter " + name)
	check(player_counts.get("raycasts", 0) > 0, "advance did not count rays")
	check(player_counts.get("shape_casts", 0) > 0, "advance did not count capsule queries")
	check(player_counts.get("record_lookups", 0) > 0, "advance did not include game record queries")
	print("PRF_14_PLAYER_COUNTS " + JSON.stringify(player_counts))
	print("PRF_14_COUNTS " + JSON.stringify(counts))
	scene.queue_free()
	await process_frame
	print("RESULT: PASS" if failures == 0 else "RESULT: FAIL")
	quit(0 if failures == 0 else 1)
