# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://r4_giant_behaviour_test.gd"

## An authored NPC strike must cause a host-confirmed damaging contact.
func _measure() -> void:
	startup_position = Vector3(-125, -800, 120)
	await super._start()
	if not loaded:
		return
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
	_place(887.0)
	player.call("debug_view_command", "tcai")
	await _sample("warning", 240, false, true)
	if not bool(_state().get("warning_complete", false)):
		failures += 1
	player.call("debug_view_command", "prid 00000014")
	player.call("debug_view_command", "setav health 1000")
	var health_before := float(player.call("gameplay_actor_value", 0, 0x14, "health"))
	_place(300.0)
	var attacked := false
	var minimum_health := health_before
	for frame in 600:
		player.call("advance", STEP)
		await process_frame
		attacked = attacked or str(giant_node.call("active_clips")).to_lower().contains("attack")
		minimum_health = minf(minimum_health, float(player.call("gameplay_actor_value", 0, 0x14, "health")))
		if frame % 60 == 59:
			print("R4_STRIKE_SAMPLE ", JSON.stringify({"frame": frame, "clips": giant_node.call("active_clips"), "state": _state(), "health": minimum_health}))
	if health_before <= 0.0 or not attacked or minimum_health >= health_before - 1.0:
		failures += 1
	print("R4_STRIKE_RESULT ", JSON.stringify({"failures": failures, "attack_clip": attacked, "health_before": health_before, "health_after": minimum_health}))
	quit(0 if failures == 0 else 1)
