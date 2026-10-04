extends "res://walkthrough.gd"

## TST-18 walkthrough workload within PRF-16's fixed headless suite.
const Metrics := preload("res://frame_budget_metrics.gd")
const STEPS := 3600
const DT := 1.0 / 60.0
var metrics := Metrics.new()
var scenario := ""
var measurement_started := 0
var exterior_actors := 0
var runtime_start := {}


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)


func _start() -> void:
	var args := OS.get_cmdline_user_args()
	if args.size() != 1 or args[0] not in ["bannered-mare", "walk-out"] or DisplayServer.get_name() != "headless":
		_fail("requires headless and one fixed scenario name")
		return
	scenario = args[0]
	Engine.max_fps = 60
	await super._start()
	if loaded:
		_configure_player()
		if player.get("auto_step"):
			_fail("manual stepping configuration failed")
			return
		await _run_scenario()


func _process(_delta: float) -> bool:
	return false


func _fail(message: String) -> void:
	printerr("PRF-16 ERROR: ", message)
	quit(1)


func _key(code: Key, pressed: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = code
	event.physical_keycode = code
	event.pressed = pressed
	Input.parse_input_event(event)
	Input.flush_buffered_events()


func _step(door: Dictionary = {}) -> bool:
	if not player.call("begin_budget_frame"):
		_fail("timing window already open")
		return false
	var activated := true
	if not door.is_empty():
		activated = player.call("activate_reference", door.file, door.object)
	player.call("advance", DT)
	var sample: Dictionary = player.call("end_budget_frame")
	metrics.observe(sample)
	if not activated:
		_fail("authored load door activation failed")
	return activated


func _walk_out() -> bool:
	var chosen := {}
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked and door.has("return_position"):
			chosen = door
			break
	if chosen.is_empty():
		_fail("no unlocked return-linked load door")
		return false
	var target: Dictionary = player.call("gameplay_reference_pose", chosen.file, chosen.object)
	var landing: Vector3 = chosen.return_position
	var yaw: float = chosen.return_yaw
	var start := landing + Vector3(sin(yaw), cos(yaw), 0.0) * 96.0
	if player.call("enter_cell", "WhiterunBanneredMare", start.x, start.y, start.z + 32.0) <= 0:
		_fail("walk approach collision failed")
		return false
	for i in 120:
		player.call("advance", DT)
		await process_frame
	var before: Vector3 = player.call("position")
	measurement_started = Time.get_ticks_usec()
	runtime_start = player.call("budget_runtime_status")
	_key(KEY_W, true)
	var reached := false
	for i in 600:
		var feet: Vector3 = player.call("position")
		var offset: Vector3 = target.position - feet
		if Vector2(offset.x, offset.y).length() <= 80.0:
			reached = true
			break
		player.call("set_facing", atan2(offset.x, offset.y), 0.0)
		if not _step():
			_key(KEY_W, false)
			return false
		await process_frame
	_key(KEY_W, false)
	var after: Vector3 = player.call("position")
	if not reached or Vector2(after.x - before.x, after.y - before.y).length() < 32.0:
		_fail("scripted movement did not reach the authored door")
		return false
	var interior: String = player.call("gameplay_cell")
	if not _step(chosen):
		return false
	var outside: String = player.call("gameplay_cell")
	var stats: Dictionary = streamer.call("last_stats")
	var runtime: Dictionary = player.call("budget_runtime_status")
	# CONTEXT: The exit marker and rendered world identify the destination despite grid refocusing.
	var far_pose: Dictionary = player.call("gameplay_reference_pose", chosen.far_file, chosen.far_object)
	if outside == interior or outside == "" or far_pose.is_empty() or runtime.world_editor_id != "WhiterunWorld" or streamer.call("instance_count") <= 0:
		_fail("load door did not replace the Bannered Mare with its exterior")
		return false
	print("PRF-16 walk ", before, " -> ", after, " cell=", outside, " stats=", stats)
	exterior_actors = player.call("sync_contact_actors")
	if exterior_actors < 5:
		_fail("Whiterun NPC workload missing")
		return false
	return true


func _run_scenario() -> void:
	if not player.call("prepare_ai_planning_check"):
		_fail("navigation preparation failed")
		return
	var actors: int = player.call("sync_contact_actors")
	if actors < 5:
		_fail("Bannered Mare NPC workload missing")
		return
	for i in 120:
		player.call("advance", DT)
		await process_frame
	measurement_started = Time.get_ticks_usec()
	runtime_start = player.call("budget_runtime_status")
	if scenario == "walk-out" and not await _walk_out():
		return
	for i in STEPS:
		if not _step():
			return
		await process_frame
		if i % 600 == 599:
			print("PRF-16 progress ", scenario, " steps=", i + 1)
	var elapsed := (Time.get_ticks_usec() - measurement_started) / 1000000.0
	var runtime_end: Dictionary = player.call("budget_runtime_status")
	var evaluations: int = runtime_end.evaluations - runtime_start.evaluations
	var ticks: int = runtime_end.ticks - runtime_start.ticks
	var seconds: float = runtime_end.seconds - runtime_start.seconds
	if evaluations <= 0 or ticks < STEPS or seconds < 59.999 or runtime_end.navmesh_triangles <= 0:
		_fail("NPC packages, navigation or 60 simulation seconds did not run: " + str(runtime_end))
		return
	metrics.report({
		"scenario": scenario, "elapsed_s": elapsed, "simulated_s": snappedf(seconds, 0.000001),
		"game_ticks": ticks,
		"actors": actors, "evaluations": evaluations, "navmesh": true,
		"walked": scenario == "walk-out", "cell": player.call("gameplay_cell"),
		"exterior_actors": exterior_actors,
		"world_editor_id": runtime_end.world_editor_id,
	})
	print("prf_16_", scenario.replace("-", "_"), "_installed_ai: PASS")
	quit(0)
