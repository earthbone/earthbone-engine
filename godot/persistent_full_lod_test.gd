# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Flagged full models use physical near cells and bounded visible demand.
var failures := 0


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)


func _process(_delta: float) -> bool:
	return false


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FULL_LOD_FAIL ", message)


func _drawn() -> Dictionary:
	var drawn := {}
	for surface: Dictionary in streamer.call("play_path_surfaces"):
		if surface.get("kind", "") == "lod":
			continue
		_check(surface.rid.is_valid(), "live RenderingServer RID")
		for reference: String in surface.get("references", PackedStringArray()):
			drawn[reference] = true
	return drawn


func _start() -> void:
	await super._start()
	if not loaded:
		quit(1)
		return
	var expected: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(OS.get_environment("SOE_FULL_LOD_EXPECT")))
	var exited := false
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			exited = player.call("activate_reference", door.file, door.object)
			break
	_check(exited, "authored Bannered Mare exit")
	player.call("advance", 0.0)
	for _frame in range(90):
		player.call("advance", 1.0 / 60.0)
		await process_frame
	_check(not player.call("door_fade_active"), "inn exit fade completed")
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "Whiterun gate found")
	if gate.is_empty():
		quit(1)
		return
	_check(player.call("queue_player_input", {"action": "move_to", "file": gate.file, "object": gate.object, "offset": Vector3(0, 200, 0), "match_rotation": true}), "gate approach")
	_check(player.call("step_player_input"), "gate approach applied")
	_check(player.call("activate_reference", gate.file, gate.object), "authored gate crossing")
	player.call("advance", 0.0)
	_check(player.call("detection_worldspace") == "Tamriel", "Tamriel at gate")
	for _frame in range(90):
		player.call("advance", 1.0 / 60.0)
		await process_frame
	_check(not player.call("door_fade_active"), "gate fade completed")
	for _frame in range(140):
		player.call("advance", 0.0)
		await process_frame
	await process_frame
	await RenderingServer.frame_post_draw
	var drawn := _drawn()
	for reference: String in expected.markers:
		_check(not drawn.has(reference), "editor marker must stay invisible " + reference)
	var count := 0
	for reference: String in expected.all:
		count += int(drawn.has(reference))
	for reference: String in streamer.call("distant_references"):
		_check(expected.all.has(reference), "selected enabled full model " + reference)
		_check(drawn.has(reference), "admitted full model missing " + reference)
	_check(count > 0 and count < expected.all.size(), "bounded visible demand replaces whole-world preparation")
	for reference: String in expected.near:
		print("FULL_LOD_GATE ", reference, " drawn=", drawn.has(reference))
	print("FULL_LOD_WORLD expected=", expected.all.size(), " drawn=", count, " near=", expected.near.size())
	var memory := {}
	for system: String in player.call("system_frame_breakdown"):
		var sample: Dictionary = player.call("system_frame_breakdown")[system]
		if sample.has("live_bytes"):
			memory[system] = sample.live_bytes
	print("FULL_LOD_MEMORY ", JSON.stringify({"rust": memory, "godot_static": Performance.get_monitor(Performance.MEMORY_STATIC), "plugin_records": streamer.call("plugin_record_memory"), "chargen": streamer.call("chargen_resident_bytes"), "lod_cache": streamer.call("lod_cache_memory")}))
	_check(player.call("activate_reference", gate.far_file, gate.far_object), "return to Whiterun")
	player.call("advance", 0.0)
	_check(player.call("detection_worldspace") == "WhiterunWorld", "worldspace changed")
	for _frame in range(90):
		player.call("advance", 1.0 / 60.0)
		await process_frame
	_check(not player.call("door_fade_active"), "return fade completed")
	await process_frame
	drawn = _drawn()
	var retained := 0
	for reference: String in expected.all:
		retained += int(drawn.has(reference))
	_check(retained == 0, "Tamriel full models released")
	print("FULL_LOD_RELEASE retained=", retained)
	print("FULL_LOD_RESULT ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
