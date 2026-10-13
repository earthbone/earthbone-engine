# SPDX-License-Identifier: GPL-3.0-or-later

# Ported from Mudcrab (github.com/Mudcrab-Team/mudcrab,
# docs/research/streaming-comparison-results.md at PR 212, 97be55d), MIT License,
# Copyright (c) the Mudcrab contributors
extends "res://walkthrough.gd"

## Fixed-route submission observations follow Mudcrab PRs 206 and 212.
const HITCH_MS := 33.4
const WALK_SECONDS := 60.0
var started := 0
var previous := 0
var route_frames := 0
var hitches := 0
var worst := 0.0
var collection_us := 0
var first_demand := {}
var drawable_ms := {}
var stops: Array[String] = []

func _configure_player() -> void:
	player.set("capture_mouse", false)

func _start() -> void:
	startup_position = Vector3(-125.0, -800.0, 120.0)
	Engine.max_fps = 60
	await super._start()
	if not loaded:
		return
	var callback := Callable(system_overlay, "_write_on_player_exit")
	if player.is_connected("tree_exiting", callback):
		player.disconnect("tree_exiting", callback)
	await create_timer(5.0).timeout
	started = Time.get_ticks_usec()
	previous = started
	var exit_door: Dictionary = {}
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked and door.get("far_worldspace", "") == "WhiterunWorld":
			exit_door = door
			break
	if exit_door.is_empty():
		_end(false, "missing inn exit")
		return
	var exit_pose: Dictionary = player.call("gameplay_reference_pose", exit_door.file, exit_door.object)
	if exit_pose.is_empty() or not await _walk(exit_pose.position, "inn_exit"):
		_end(false, "inn exit walk")
		return
	if not player.call("activate_reference", exit_door.file, exit_door.object):
		_end(false, "inn exit activation")
		return
	await process_frame
	stops.append("city_entry")
	var city_start: Vector3 = player.call("position")
	var gate: Dictionary = player.call("detection_gate_reference")
	var gate_pose: Dictionary = player.call("gameplay_reference_pose", gate.get("file", -1), gate.get("object", -1))
	if gate_pose.is_empty() or not await _walk(gate_pose.position, "gate"):
		_end(false, "gate walk")
		return
	if not await _walk(city_start, "city_return"):
		_end(false, "city return walk")
		return
	if not player.call("activate_reference", exit_door.far_file, exit_door.far_object):
		_end(false, "inn return activation")
		return
	await process_frame
	stops.append("inn_return")
	_end(true, "completed")

func _process(delta: float) -> bool:
	super._process(delta)
	if started == 0:
		return false
	var now := Time.get_ticks_usec()
	var frame_ms := (now - previous) / 1000.0
	previous = now
	route_frames += 1
	worst = maxf(worst, frame_ms)
	if frame_ms > HITCH_MS:
		hitches += 1
	if route_frames % 8 == 0:
		var begin := Time.get_ticks_usec()
		var position: Vector3 = player.call("position")
		var key := "%s:%d,%d" % [player.call("detection_worldspace"), floori(position.x / 4096.0), floori(position.y / 4096.0)]
		if not first_demand.has(key):
			first_demand[key] = now
		var batches: Dictionary = streamer.call("exterior_cell_batches", floori(position.x / 4096.0), floori(position.y / 4096.0))
		if batches.instances > 0 and not drawable_ms.has(key):
			drawable_ms[key] = (now - int(first_demand[key])) / 1000.0
		collection_us += Time.get_ticks_usec() - begin
	return false

func _key(pressed: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = KEY_W
	event.physical_keycode = KEY_W
	event.pressed = pressed
	Input.parse_input_event(event)
	Input.flush_buffered_events()

func _walk(target: Vector3, stop: String) -> bool:
	var navigation: Dictionary = player.call("play_path_navigation_details", target)
	var points: PackedVector3Array = navigation.points
	if points.size() < 2:
		return false
	var begin := Time.get_ticks_msec()
	for point in points:
		while true:
			var position: Vector3 = player.call("position")
			var difference := point - position
			if Vector2(difference.x, difference.y).length() < 36.0:
				break
			if Time.get_ticks_msec() - begin > int(WALK_SECONDS * 1000.0):
				_key(false)
				return false
			player.call("set_facing", atan2(difference.x, difference.y), 0.0)
			_key(true)
			await process_frame
	_key(false)
	stops.append(stop)
	print("STREAM_ROUTE_STOP ", JSON.stringify({"stop": stop, "position": str(player.call("position"))}))
	return true

func _end(passed: bool, reason: String) -> void:
	_key(false)
	print("STREAM_ROUTE_RESULT ", JSON.stringify({"pass": passed, "reason": reason, "stops": stops,
		"hitch_threshold_ms": HITCH_MS, "hitches": hitches, "frames": route_frames, "worst_ms": worst,
		"focus_to_submission_ms": drawable_ms, "observed_demand": first_demand.size(),
		"probe_collection_ms": collection_us / 1000.0, "elapsed_s": (Time.get_ticks_usec() - started) / 1000000.0}))
	quit(0 if passed else 1)
