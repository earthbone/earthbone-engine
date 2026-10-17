# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Persistent full-model residency follows the vanilla header flags.
var failures := 0
var frame_peak := {}
var current_phase := "startup"


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)


func _process(_delta: float) -> bool:
	var rss := _status_mib("VmRSS")
	if rss > frame_peak.get("rss_mib", 0.0):
		frame_peak = {"phase": current_phase, "rss_mib": rss, "hwm_mib": _status_mib("VmHWM"), "godot": _monitors()}
	return false


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FULL_LOD_FAIL ", message)


## A load door plays its open animation and then prepares its destination across
## host frames, so a check of the new location waits until both end.
func _settle_location(label: String) -> void:
	var frames := 0
	while (player.call("is_load_door_pending") or player.call("is_location_preparing")) and frames < 1800:
		player.call("advance", 1.0 / 60.0)
		await process_frame
		frames += 1
	_check(player.call("gameplay_location_ready"), label + " location ready after " + str(frames) + " frames")


func _drawn() -> Dictionary:
	var drawn := {}
	for surface: Dictionary in streamer.call("play_path_surfaces"):
		if surface.get("kind", "") == "lod":
			continue
		_check(surface.rid.is_valid(), "live RenderingServer RID")
		for reference: String in surface.get("references", PackedStringArray()):
			drawn[reference] = true
	return drawn


func _status_mib(key: String) -> float:
	var file := FileAccess.open("/proc/self/status", FileAccess.READ)
	if file == null:
		return 0.0
	while not file.eof_reached():
		var line := file.get_line()
		if line.begins_with(key + ":"):
			return float(line.get_slice(":", 1).strip_edges().get_slice(" ", 0)) / 1024.0
	return 0.0


func _monitors() -> Dictionary:
	return {
		"static": Performance.get_monitor(Performance.MEMORY_STATIC),
		"video": Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED),
		"texture": Performance.get_monitor(Performance.RENDER_TEXTURE_MEM_USED),
		"buffer": Performance.get_monitor(Performance.RENDER_BUFFER_MEM_USED),
		"objects": Performance.get_monitor(Performance.OBJECT_COUNT),
		"resources": Performance.get_monitor(Performance.OBJECT_RESOURCE_COUNT),
		"nodes": Performance.get_monitor(Performance.OBJECT_NODE_COUNT),
		"orphans": Performance.get_monitor(Performance.OBJECT_ORPHAN_NODE_COUNT),
	}


## Report process RSS next to Godot monitors and host payload ownership.
func _phase(label: String) -> void:
	current_phase = label
	print("FULL_LOD_PHASE ", JSON.stringify({"phase": label, "rss_mib": _status_mib("VmRSS"), "anon_mib": _status_mib("RssAnon"), "file_mib": _status_mib("RssFile"), "shmem_mib": _status_mib("RssShmem"), "hwm_mib": _status_mib("VmHWM"), "godot": _monitors(), "attribution": streamer.call("memory_attribution"), "actors": _actor_memory(), "lod_cache": streamer.call("lod_cache_memory"), "collision": player.call("collision_memory_attribution"), "prefetch": player.call("interior_prefetch_status"), "npc_cache": player.call("npc_texture_memory_attribution"), "plugin_records": streamer.call("plugin_record_memory")}))


func _actor_memory() -> Dictionary:
	var result := {"nodes": 0, "cpu_mesh_allocation_bytes": 0, "cpu_expression_bytes": 0, "cpu_image_allocation_bytes": 0}
	for child: Node in player.get_children():
		if child.get_class() == "SkinnedActorNode":
			result.nodes += 1
			var memory: Dictionary = child.call("memory_attribution")
			for key: String in result:
				if key != "nodes":
					result[key] = int(result[key]) + int(memory.get(key, 0))
	return result


func _start() -> void:
	await super._start()
	if not loaded:
		quit(1)
		return
	_phase("after_inn")
	var expected: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(OS.get_environment("SOE_FULL_LOD_EXPECT")))
	var exited := false
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			exited = player.call("activate_reference", door.file, door.object)
			break
	_check(exited, "authored Bannered Mare exit")
	player.call("advance", 0.0)
	await _settle_location("inn exit")
	for _frame in range(90):
		player.call("advance", 1.0 / 60.0)
		await process_frame
	_check(not player.call("door_fade_active"), "inn exit fade has completed")
	_phase("after_exit")
	var gate: Dictionary = player.call("detection_gate_reference")
	_check(not gate.is_empty(), "Whiterun gate found")
	if gate.is_empty():
		quit(1)
		return
	_check(player.call("queue_player_input", {"action": "move_to", "file": gate.file, "object": gate.object, "offset": Vector3(0, 200, 0), "match_rotation": true}), "gate approach")
	_check(player.call("step_player_input"), "gate approach applied")
	_check(player.call("activate_reference", gate.file, gate.object), "authored gate crossing")
	player.call("advance", 0.0)
	await _settle_location("gate crossing")
	_check(player.call("detection_worldspace") == "Tamriel", "Tamriel at gate, saw \"" + str(player.call("detection_worldspace")) + "\" in cell " + str(player.call("gameplay_cell")))
	for _frame in range(90):
		player.call("advance", 1.0 / 60.0)
		await process_frame
	_check(not player.call("door_fade_active"), "gate fade has completed")
	_phase("gate_settled")
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
	_report_memory()
	_phase("after_gate")
	var position: Vector3 = player.call("position")
	var peak := Vector3(float(expected.peak[0]), float(expected.peak[1]), float(expected.peak[2]))
	var delta := peak - position
	player.call("set_facing", atan2(delta.x, delta.y), atan2(delta.z, Vector2(delta.x, delta.y).length()) * 0.6)
	await _measure("gate", count)
	# CONTEXT: RND-27's outside-gate viewpoint clears the roof and player geometry.
	var gameplay_camera := root.get_camera_3d()
	var camera := Camera3D.new()
	root.add_child(camera)
	camera.position = Vector3(18000, -3272, 17000) / 70.0
	camera.far = 4000.0
	camera.look_at(Vector3(peak.x, -3272 + (peak.z + 3272) * 0.6, -peak.y) / 70.0)
	camera.make_current()
	print("FULL_LOD_CAMERA ", JSON.stringify({"feet": str(position), "peak": str(peak), "camera": str(camera.global_position), "forward": str(-camera.global_basis.z), "third_person": player.call("is_third_person"), "native": player.call("camera_state")}))
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
	var sky := terrain.get_pixel(320, 0)
	_check(terrain.get_pixel(0, 0).is_equal_approx(sky) and terrain.get_pixel(639, 0).is_equal_approx(sky), "sky spans the top of the landmark control")
	for y in range(20, 200):
		for x in range(80, 560):
			var a := landmark.get_pixel(x, y)
			var b := terrain.get_pixel(x, y)
			var sky_distance := absf(sky.r-b.r) + absf(sky.g-b.g) + absf(sky.b-b.b)
			if sky_distance < 0.03 and absf(a.r-b.r) + absf(a.g-b.g) + absf(a.b-b.b) > 0.12:
				silhouette_pixels += 1
	_check(silhouette_pixels >= 500, "distant mountain clears terrain ridge")
	print("FULL_LOD_LANDMARK pixels=", silhouette_pixels, " threshold=500")
	_phase("landmark_done")
	streamer.call("set_object_lod_visible", true)
	gameplay_camera.make_current()
	camera.queue_free()
	_check(player.call("activate_reference", gate.far_file, gate.far_object), "return to Whiterun")
	player.call("advance", 0.0)
	await _settle_location("return to Whiterun")
	for _frame in range(90):
		player.call("advance", 1.0 / 60.0)
		await process_frame
	_check(not player.call("door_fade_active"), "city fade has completed")
	_check(player.call("detection_worldspace") == "WhiterunWorld", "worldspace changed, saw \"" + str(player.call("detection_worldspace")) + "\" in cell " + str(player.call("gameplay_cell")))
	await process_frame
	drawn = _drawn()
	var retained := 0
	for reference: String in expected.all:
		retained += int(drawn.has(reference))
	_check(retained == 0, "Tamriel full models released")
	_check(streamer.call("distant_references").is_empty(), "distant CPU and GPU ownership released")
	print("FULL_LOD_RELEASE retained=", retained)
	_phase("after_release")
	var city_count := 0
	for reference: String in expected.city:
		city_count += int(drawn.has(reference))
	_check(city_count == expected.city.size(), "every authored Whiterun flagged model drawn")
	await _measure("city", city_count)
	print("FULL_LOD_RESULT ", "PASS" if failures == 0 else "FAIL")
	_phase("end")
	print("FULL_LOD_FRAME_PEAK ", JSON.stringify(frame_peak))
	quit(0 if failures == 0 else 1)


func _report_memory() -> void:
	var memory := {}
	var breakdown: Dictionary = player.call("system_frame_breakdown")
	for system: String in breakdown:
		var sample: Dictionary = breakdown[system]
		if sample.has("live_bytes"):
			memory[system] = sample.live_bytes
	print("FULL_LOD_MEMORY ", JSON.stringify({"rust": memory, "godot_static": Performance.get_monitor(Performance.MEMORY_STATIC), "video_bytes": Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED), "texture_bytes": Performance.get_monitor(Performance.RENDER_TEXTURE_MEM_USED), "plugin_records": streamer.call("plugin_record_memory"), "chargen": streamer.call("chargen_resident_bytes"), "lod_cache": streamer.call("lod_cache_memory")}))


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
