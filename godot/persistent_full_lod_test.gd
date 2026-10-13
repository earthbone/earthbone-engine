# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Persistent full-model residency follows the vanilla header flags.
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
	_check(not player.call("door_fade_active"), "gate fade has completed")
	for _frame in range(140):
		player.call("advance", 0.0)
		await process_frame
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
	print("FULL_LOD_WORLD expected=", expected.all.size(), " drawn=", count, " near=", expected.near.size())
	var position: Vector3 = player.call("position")
	var peak := Vector3(float(expected.peak[0]), float(expected.peak[1]), float(expected.peak[2]))
	var delta := peak - position
	player.call("set_facing", atan2(delta.x, delta.y), atan2(delta.z, Vector2(delta.x, delta.y).length()) * 0.6)
	await _measure("gate", count)
	root.use_occlusion_culling = true
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.15, 0.25, 0.45)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color.WHITE
	environment.ambient_light_energy = 0.8
	root.world_3d.environment = environment
	for _frame in range(8):
		await process_frame
	await RenderingServer.frame_post_draw
	var landmark := root.get_texture().get_image()
	var directory := OS.get_environment("SOE_VISIBILITY_CAPTURE")
	_check(landmark.save_png(directory.path_join("gate-landmark.png")) == OK, "save landmark over terrain")
	streamer.call("set_object_lod_visible", false)
	for _frame in range(8):
		await process_frame
	await RenderingServer.frame_post_draw
	var terrain := root.get_texture().get_image()
	_check(terrain.save_png(directory.path_join("gate-terrain.png")) == OK, "save terrain control")
	var silhouette_pixels := 0
	var sky := terrain.get_pixel(0, 0)
	for y in range(20, 200):
		for x in range(80, 560):
			var a := landmark.get_pixel(x, y)
			var b := terrain.get_pixel(x, y)
			var sky_distance := absf(sky.r-b.r) + absf(sky.g-b.g) + absf(sky.b-b.b)
			if sky_distance < 0.03 and absf(a.r-b.r) + absf(a.g-b.g) + absf(a.b-b.b) > 0.12:
				silhouette_pixels += 1
	_check(silhouette_pixels >= 500, "distant mountain clears terrain ridge")
	print("FULL_LOD_LANDMARK pixels=", silhouette_pixels, " threshold=500")
	streamer.call("set_object_lod_visible", true)
	var memory := {}
	for system: String in player.call("system_frame_breakdown"):
		var sample: Dictionary = player.call("system_frame_breakdown")[system]
		if sample.has("live_bytes"):
			memory[system] = sample.live_bytes
	print("FULL_LOD_MEMORY ", JSON.stringify({"rust": memory, "godot_static": Performance.get_monitor(Performance.MEMORY_STATIC), "video_bytes": Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED), "texture_bytes": Performance.get_monitor(Performance.RENDER_TEXTURE_MEM_USED), "plugin_records": streamer.call("plugin_record_memory"), "chargen": streamer.call("chargen_resident_bytes"), "lod_cache": streamer.call("lod_cache_memory")}))
	_check(player.call("activate_reference", gate.far_file, gate.far_object), "return to Whiterun")
	player.call("advance", 0.0)
	for _frame in range(90):
		player.call("advance", 1.0 / 60.0)
		await process_frame
	_check(not player.call("door_fade_active"), "city fade has completed")
	_check(player.call("detection_worldspace") == "WhiterunWorld", "worldspace changed")
	await process_frame
	drawn = _drawn()
	var retained := 0
	for reference: String in expected.all:
		retained += int(drawn.has(reference))
	_check(retained == 0, "Tamriel full models released")
	_check(streamer.call("distant_references").is_empty(), "distant CPU and GPU ownership released")
	print("FULL_LOD_RELEASE retained=", retained)
	var city_count := 0
	for reference: String in expected.city:
		city_count += int(drawn.has(reference))
	_check(city_count == expected.city.size(), "every authored Whiterun flagged model drawn")
	await _measure("city", city_count)
	print("FULL_LOD_RESULT ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)


func _measure(location: String, count: int) -> void:
	var calls := {}
	for enabled: bool in [false, true]:
		root.use_occlusion_culling = enabled
		for _frame in range(12):
			await process_frame
		await RenderingServer.frame_post_draw
		calls["occlusion_on" if enabled else "occlusion_off"] = int(Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME))
	_check(calls.occlusion_on <= calls.occlusion_off, location + " occlusion does not increase draw calls")
	var occluders: Dictionary = streamer.call("occluder_counts")
	_check(occluders.cells + occluders.terrain > 0, location + " live conservative occluders")
	var values := {"location": location, "loaded": count, "distant": streamer.call("distant_references").size(), "draw_calls": calls, "occluders": occluders}
	print("FULL_LOD_MEASURE ", JSON.stringify(values))
	var file := FileAccess.open(OS.get_environment("SOE_VISIBILITY_CAPTURE").path_join(location + ".json"), FileAccess.WRITE)
	_check(file != null, "measurement output")
	if file != null:
		file.store_string(JSON.stringify(values))
