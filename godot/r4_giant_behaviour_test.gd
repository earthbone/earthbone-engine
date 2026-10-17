# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Record-radius warning and approach through the production free walk.
const STEP := 1.0 / 60.0
var failures := 0
var giant := {}
var giant_node: Node
var giant_id := ""

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
	print("R4_WARNING combat disabled=", player.call("debug_view_command", "tcai"))
	var camp := ""
	for cell: Dictionary in streamer.call("editor_cell_tree"):
		if not cell.interior and str(cell.name).to_lower().contains("bleakwindbasin"):
			camp = cell.name
			break
	if camp.is_empty():
		quit(1)
		return
	print("R4_WARNING travel=", player.call("console_execute", "coc " + camp))
	player.call("advance", 0.0)
	while player.call("is_location_preparing"):
		await process_frame
	for _frame in 180:
		player.call("advance", STEP)
		await process_frame
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
				giant_node = actor
				break
		if not giant.is_empty():
			break
	if giant.is_empty():
		quit(1)
		return
	giant_id = str(giant.file) + ":%06X" % int(giant.object)
	var source := _state()
	print("R4_WARNING_SOURCE ", JSON.stringify(source))
	for query in ["help Aggro 4", "help Warning 4", "help NPCGiant 4", "help WarnAttack 4"]:
		print("R4_WARNING_SETTINGS ", player.call("console_execute", query))
	var warn := float(source.warn_radius)
	var warn_attack := float(source.warn_attack_radius)
	var attack := float(source.attack_radius)
	if not (warn > warn_attack and warn_attack > attack and attack > 0.0):
		quit(1)
		return
	_place(warn + 256.0)
	print("R4_WARNING combat enabled=", player.call("debug_view_command", "tcai"))
	await _sample("outside", 120, false, false)
	_place((warn + warn_attack) * 0.5)
	await _sample("warning", 120, false, true)
	_place(attack * 0.8)
	await _sample("attack", 600, true, false)
	var approach := float(_state().get("combat_distance", 0.0))
	_place(approach * 1.75)
	await _sample("approach", 600, true, false)
	var final := _state()
	var separation := INF
	for body: Dictionary in player.call("npc_grounding_status"):
		if body.file == giant.file and body.object == giant.object:
			var offset: Vector3 = body.position - player.call("position")
			separation = Vector2(offset.x, offset.y).length()
	if approach <= 4.0 or not bool(final.arrived) or separation < approach - 16.0 or separation > approach + 32.0:
		failures += 1
	print("R4_WARNING_APPROACH ", JSON.stringify({"state": final, "separation_units": separation}))
	print("R4_WARNING_RESULT failures=", failures)
	quit(0 if failures == 0 else 1)

func _place(distance: float) -> void:
	player.call("debug_view_command", "prid %08X" % int(giant.object))
	player.call("debug_view_command", "setangle z 90")
	player.call("set_facing", -PI * 0.5, 0.0)
	var queued: bool = player.call("queue_player_input", {"action": "move_to", "file": giant.file, "object": giant.object, "offset": Vector3(distance, 0, 0), "match_rotation": false})
	if not queued or not bool(player.call("step_player_input")):
		failures += 1

func _state() -> Dictionary:
	for state: Dictionary in player.call("npc_package_states"):
		if state.actor == giant_id or (giant_id.is_empty() and state.actor == str(giant.file) + ":%06X" % int(giant.object)):
			return state
	return {}

func _sample(label: String, count: int, combat: bool, warning: bool) -> void:
	var warned := false
	var engaged := false
	var peak_detection := -INF
	var sounds_before := int(_state().get("aware_sound_starts", 0))
	var previous: Vector3 = player.call("position")
	var travelled := 0.0
	for frame in count:
		if warning and frame % 15 == 0:
			_key(KEY_D, frame % 30 == 0)
			_key(KEY_A, frame % 30 != 0)
		player.call("advance", STEP)
		await process_frame
		var state := _state()
		var position: Vector3 = player.call("position")
		travelled += previous.distance_to(position)
		previous = position
		peak_detection = maxf(peak_detection, float(player.call("detection_value", giant.file, giant.object, 0, 0x14)))
		engaged = engaged or bool(state.get("in_combat", false))
		var clips: Array = giant_node.call("active_clips")
		warned = warned or JSON.stringify(clips).to_lower().contains("aggrowarning")
		if frame % 60 == 59:
			var placement: Transform3D = giant_node.call("play_path_observation").placement
			var giant_units := Vector3(placement.origin.x, -placement.origin.z, placement.origin.y) / 0.0142875
			print("R4_WARNING_SAMPLE ", JSON.stringify({"phase": label, "frame": frame, "state": state, "clips": clips, "separation_units": giant_units.distance_to(position), "player": position, "giant": giant_units}))
	_key(KEY_D, false)
	_key(KEY_A, false)
	var sounds := int(_state().get("aware_sound_starts", 0)) - sounds_before
	if engaged != combat or (warning and (not warned or peak_detection <= 0.0 or sounds < 1 or travelled < 16.0)):
		failures += 1
	print("R4_WARNING_PHASE ", JSON.stringify({"phase": label, "engaged": engaged, "warned": warned, "peak_detection": peak_detection, "aware_sound_starts": sounds, "player_units": travelled}))

func _key(code: int, pressed: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = code
	event.physical_keycode = code
	event.pressed = pressed
	Input.parse_input_event(event)
	Input.flush_buffered_events()
