extends "res://walkthrough.gd"

## HST-30's npc.grounded check, with the 32-unit limit fixed before the run.
const DT := 1.0 / 60.0
const BELOW_GROUND_LIMIT := 32.0
var failures := 0
var checked := 0


func _initialize() -> void:
	startup_position = Vector3(-125, -800, 120)
	_watchdog()
	super._initialize()


func _watchdog() -> void:
	await create_timer(300.0).timeout
	printerr("FAIL npc.grounded: play-path watchdog")
	quit(1)


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	player.set("exterior_lod_full_res_radius", 2)
	player.set("exterior_lod_radius", 3)


func _process(_delta: float) -> bool:
	return false


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL npc.grounded: ", message)


func _stop(name: String) -> void:
	await process_frame
	await process_frame
	var deadline := Time.get_ticks_msec() + 90_000
	var ready := false
	while Time.get_ticks_msec() < deadline:
		player.call("advance", DT)
		await process_frame
		var counts: Dictionary = player.call("npc_visual_counts")
		if counts.expected > 0 and counts.pending == 0 and counts.drawn + counts.failed == counts.expected:
			print("NPC_ASSETS stop=", name, " counts=", counts)
			ready = true
			break
	_check(ready, name + " all resident NPCs finished loading")
	for _frame in 120:
		player.call("advance", DT)
		if _frame % 30 == 29:
			await process_frame
	var grounded: Array = player.call("npc_grounding_status")
	_check(not grounded.is_empty(), name + " has living NPC capsules")
	var supported := 0
	for actor: Dictionary in grounded:
		checked += 1
		_check(actor.position.is_finite(), name + " finite placement " + str(actor))
		_check(actor.has("ground_z") or actor.has("navmesh_z"), name + " support under NPC " + str(actor))
		if actor.has("ground_z"):
			_check(actor.below_ground <= BELOW_GROUND_LIMIT, name + " below ground " + str(actor))
		if actor.has("navmesh_z"):
			_check(actor.below_navmesh <= BELOW_GROUND_LIMIT, name + " below navmesh " + str(actor))
		print("PLAY_PATH ", JSON.stringify({"check": "npc.grounded", "row": "HST-30", "stop": name,
			"pass": (actor.has("ground_z") or actor.has("navmesh_z")) and actor.get("below_ground", 0.0) <= BELOW_GROUND_LIMIT and actor.get("below_navmesh", 0.0) <= BELOW_GROUND_LIMIT,
			"measured": actor}))
		if actor.on_ground:
			supported += 1
	_check(supported > 0, name + " NPCs stand on cell collision")
	print("TST-19 npc.grounded stop=", name, " checked=", grounded.size(), " supported=", supported,
		" timing=", player.call("npc_visual_timing"))
	await RenderingServer.frame_post_draw
	_check(root.get_texture().get_image().save_png("res://../target/hst_30_" + name + ".png") == OK,
		name + " capture saved")


func _start() -> void:
	if OS.get_environment("SKYRIM_DATA").is_empty():
		print("SKIP npc.grounded: SKYRIM_DATA unset")
		quit(0)
		return
	_check(OS.get_environment("SOE_PLAY_PATH") == "gate", "SOE_PLAY_PATH=gate")
	_check(DisplayServer.get_name() != "headless", "windowed play path")
	await super._start()
	if not loaded:
		quit(1)
		return
	system_overlay.queue_free()
	await process_frame
	_check(player.call("prepare_ai_planning_check"), "NAVM prepared")
	await _stop("bannered_mare")
	var exited := false
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			exited = player.call("activate_reference", door.file, door.object)
			break
	_check(exited, "authored Bannered Mare exit")
	player.call("advance", 0.0)
	await _stop("whiterun_city")
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "Whiterun gate found")
	if not gate.is_empty():
		_check(player.call("queue_player_input", {"action": "move_to", "file": gate.file,
			"object": gate.object, "offset": Vector3(0, 200, 0), "match_rotation": true}), "gate approach")
		_check(player.call("step_player_input"), "gate approach applied")
		await _stop("whiterun_gate")
		_check(player.call("activate_reference", gate.file, gate.object), "authored gate crossing")
		player.call("advance", 0.0)
		_check(player.call("detection_worldspace") == "Tamriel", "plains worldspace")
		await _stop("whiterun_plains")
	_check(checked > 0, "NPC survey is nonempty")
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " npc.grounded checked=", checked)
	quit(0 if failures == 0 else 1)
