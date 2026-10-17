# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Two render frames per movement tick must not restart the walking clip.
const STEP := 1.0 / 60.0
const METRES_PER_UNIT := 0.0142875

func _initialize() -> void:
	Engine.max_fps = 120
	call_deferred("_measure")

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _measure() -> void:
	startup_position = Vector3(-125, -800, 120)
	await super._start()
	if not loaded:
		return
	var actor: Node
	for _frame in 600:
		player.call("advance", STEP)
		await process_frame
		for visual: Dictionary in player.call("npc_visuals"):
			if visual.object == 0x1a66e and visual.node_id != 0:
				actor = instance_from_id(visual.node_id)
		if actor != null:
			break
	if actor == null:
		printerr("R4_NPC_CLOCK Hulda did not load")
		quit(1)
		return
	print(player.call("debug_view_command", "prid 0001a66e"))
	print(player.call("debug_view_command", "setactorsai 0"))
	var initial: Vector3 = actor.call("play_path_observation").placement.origin
	var start_y := -406.65082
	var maximum_clock := 0.0
	var walking_frames := 0
	var clips_at_end := []
	for frame in 120:
		if frame % 2 == 0:
			player.call("debug_view_command", "setpos y " + str(start_y + frame * 0.3))
			player.call("advance", STEP)
		await process_frame
		var clips: PackedStringArray = actor.call("active_clips")
		for clip: String in clips:
			if clip.to_lower().contains("walk"):
				walking_frames += 1
				maximum_clock = maxf(maximum_clock, float(clip.get_slice("@", 1)))
		if frame > 115:
			clips_at_end.append(Array(clips))
	var final: Vector3 = actor.call("play_path_observation").placement.origin
	var units := initial.distance_to(final) / METRES_PER_UNIT
	var passed := units >= 16.0 and walking_frames > 0 and maximum_clock >= 0.1
	print("R4_NPC_CLOCK ", JSON.stringify({"passed": passed, "moved_units": units, "walk_samples": walking_frames, "maximum_walk_clock": maximum_clock, "last_clips": clips_at_end, "render_hz": 120, "movement_hz": 60}))
	quit(0 if passed else 1)
