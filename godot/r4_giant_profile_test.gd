# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Fixed free walk actor update budget, measured with meshes off screen too.
const STEP := 1.0 / 60.0
const POSE_P99_US := 1000.0
const MAX_PATH_REQUESTS_PER_SECOND := 5
var failures := 0

func _initialize() -> void:
	Engine.max_fps = 60
	call_deferred("_measure")

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _measure() -> void:
	startup_position = Vector3(-125, -800, 120)
	await super._start()
	if not loaded:
		return
	player.call("debug_view_command", "prid 00000014")
	player.call("debug_view_command", "setav health 10000")
	var human_timings := {}
	var human_stages := {}
	for frame in 300:
		player.call("advance", STEP)
		await process_frame
		if frame < 120:
			continue
		for visual: Dictionary in player.call("npc_visuals"):
			if visual.node_id == 0:
				continue
			var identity := str(visual.file) + ":%06X" % int(visual.object)
			if not human_timings.has(identity):
				human_timings[identity] = []
			human_timings[identity].append(float(visual.pose_us))
			_record_stages(human_stages, identity, instance_from_id(visual.node_id))
	print("R4_HUMAN_TIMINGS ", JSON.stringify({"total": _summary(human_timings), "stages": _summary(human_stages)}))
	var camp := ""
	for cell: Dictionary in streamer.call("editor_cell_tree"):
		if not cell.interior and str(cell.name).to_lower().contains("bleakwindbasin"):
			camp = cell.name
			break
	if camp.is_empty():
		printerr("R4_GIANT no Bleakwind Basin CELL")
		quit(1)
		return
	print("R4_GIANT camp=", camp, " travel=", player.call("console_execute", "coc " + camp))
	player.call("advance", 0.0)
	while player.call("is_location_preparing"):
		await process_frame
	for _frame in 180:
		player.call("advance", STEP)
		await process_frame
	var giant := {}
	var references: Array = player.call("play_path_references")
	for visual: Dictionary in player.call("npc_visuals"):
		if visual.node_id == 0:
			continue
		var actor: Node = instance_from_id(visual.node_id)
		if not str(actor.call("skeleton_path")).to_lower().contains("/giant/"):
			continue
		for reference: Dictionary in references:
			if reference.file == visual.file and reference.object == visual.object:
				giant = reference
				break
		if not giant.is_empty():
			break
	print("R4_GIANT reference=", giant)
	if giant.is_empty():
		quit(1)
		return
	print("R4_GIANT select=", player.call("debug_view_command", "prid %08X" % int(giant.object)))
	for query in ["getav aggression", "getav confidence", "isincombat", "ShowAiList"]:
		print("R4_GIANT_RECORD ", query, " ", player.call("debug_view_command", query))
	# This is a pursuit budget, so establish hostility before moving away.
	print("R4_GIANT_HOSTILITY ", player.call("debug_view_command", "setav aggression 3"))
	var queued: bool = player.call("queue_player_input", {"action": "move_to", "file": giant.file, "object": giant.object, "offset": Vector3(700, 0, 0), "match_rotation": false})
	var applied: bool = player.call("step_player_input")
	print("R4_GIANT_PLACEMENT ", JSON.stringify({"queued": queued, "applied": applied, "player": player.call("position"), "giant": giant.position}))
	if not queued or not applied:
		quit(1)
		return
	for _frame in 180:
		player.call("advance", STEP)
		await process_frame
	print("R4_GIANT_RESIDENCY ", JSON.stringify({"player": player.call("position"), "cell": str(player.call("gameplay_cell")), "world": str(player.call("detection_worldspace"))}))
	var difference: Vector3 = giant.position - player.call("position")
	var heading := atan2(difference.x, difference.y)
	player.set("auto_step", true)
	for away in [false, true]:
		player.call("set_facing", heading + (PI if away else 0.0), 0.0)
		var key := KEY_W if away else KEY_S
		_key(key, true)
		var player_start: Vector3 = player.call("position")
		var timings := {}
		var stages := {}
		var advance_times: Array[float] = []
		var ai_times: Array[float] = []
		var peak_requests := 0
		var first_position: Vector3
		var have_position := false
		var maximum_movement := 0.0
		for frame in 300:
			await process_frame
			var systems: Dictionary = player.call("system_frame_breakdown")
			var advance_us := float(systems.get("main_thread_engine", {}).get("ms", 0.0)) * 1000.0
			advance_times.append(advance_us)
			ai_times.append(float(player.call("ai_planning_microseconds")))
			var ai: Dictionary = player.call("ai_breakdown")
			for identity: String in ai.paths:
				if identity.to_lower() == (str(giant.file) + ":%06X" % int(giant.object)).to_lower():
					peak_requests = maxi(peak_requests, int(ai.paths[identity].requests_per_second))
			for visual: Dictionary in player.call("npc_visuals"):
				if visual.node_id == 0:
					continue
				var identity := str(visual.file) + ":%06X" % int(visual.object)
				if visual.file == giant.file and visual.object == giant.object:
					if not have_position:
						first_position = visual.position
						have_position = true
					maximum_movement = maxf(maximum_movement, first_position.distance_to(visual.position) / 0.0142875)
				if not timings.has(identity):
					timings[identity] = []
				timings[identity].append(float(visual.pose_us))
				_record_stages(stages, identity, instance_from_id(visual.node_id))
				if frame % 60 == 59 and visual.file == giant.file and visual.object == giant.object:
					var actor: Node = instance_from_id(visual.node_id)
					print("R4_GIANT_FRAME ", JSON.stringify({"away": away, "frame": frame, "actor": visual, "clips": actor.call("active_clips"), "stats": actor.call("stats"), "stages": actor.call("pose_stage_timings"), "systems": systems}))
		var summary := _summary(timings)
		_key(key, false)
		var player_units := player_start.distance_to(player.call("position"))
		var giant_id := str(giant.file) + ":%06X" % int(giant.object)
		if not summary.has(giant_id) or float(summary[giant_id].p99_us) > POSE_P99_US:
			failures += 1
		var advance_summary := _summary({"advance": advance_times})
		if peak_requests > MAX_PATH_REQUESTS_PER_SECOND:
			failures += 1
		if float(advance_summary.advance.p99_us) > 33000.0 or maximum_movement < 64.0 or player_units < 64.0:
			failures += 1
		print("R4_GIANT_TIMINGS ", JSON.stringify({"away": away, "actors": summary, "stages": _summary(stages), "advance": advance_summary, "ai_tick": _summary({"ai": ai_times}), "peak_requests_per_second": peak_requests, "giant": giant_id, "moved_units": maximum_movement, "player_moved_units": player_units, "ai": player.call("ai_breakdown"), "packages": player.call("npc_package_states")}))
	print("R4_GIANT_RESULT failures=", failures)
	quit(0 if failures == 0 else 1)

func _summary(timings: Dictionary) -> Dictionary:
	var summary := {}
	for identity in timings:
		var values: Array = timings[identity]
		values.sort()
		summary[identity] = {"p50_us": values[int(values.size() * 0.5)], "p99_us": values[int(ceil(values.size() * 0.99)) - 1], "max_us": values[-1]}
	return summary

func _key(code: int, pressed: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = code
	event.physical_keycode = code
	event.pressed = pressed
	Input.parse_input_event(event)
	Input.flush_buffered_events()

func _record_stages(timings: Dictionary, identity: String, actor: Node) -> void:
	var stages: Dictionary = actor.call("pose_stage_timings")
	for stage: String in stages:
		var key := identity + "/" + stage
		if not timings.has(key):
			timings[key] = []
		timings[key].append(float(stages[stage]))
