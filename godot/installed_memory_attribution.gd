extends "res://walkthrough.gd"

## Fixed inn and gate captures. Byte totals describe payloads, not driver heaps.
const REPEATS := 3
const CACHE_LIMIT := 64 * 1024 * 1024
const STABLE_GROWTH := 1024 * 1024
var failures := 0
var snapshots: Array[Dictionary] = []


func _process(_delta: float) -> bool:
	return false


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("MEMORY FAIL: ", message)


func _move(reference: Dictionary) -> void:
	_check(player.call("queue_player_input", {"action": "move_to", "file": reference.file, "object": reference.object, "offset": Vector3.ZERO, "match_rotation": true}), "move to authored reference")
	_check(player.call("step_player_input"), "apply move")
	player.call("advance", 0.0)


func _snapshot(location: String, cycle: int) -> Dictionary:
	for i in 120:
		player.call("advance", 1.0 / 60.0)
		await process_frame
		var npcs: Dictionary = player.call("npc_visual_counts")
		if i >= 30 and npcs.get("pending", 0) == 0:
			break
	await RenderingServer.frame_post_draw
	var row := {"location": location, "cycle": cycle, "streamer": streamer.call("memory_attribution"), "actors": []}
	var texture_rids := {}
	var repeated_paths := 0
	for child: Node in player.get_children():
		if child.get_class() == "SkinnedActorNode":
			var actor_memory: Dictionary = child.call("memory_attribution")
			row.actors.append(actor_memory)
			if OS.get_environment("SOE_MEMORY_BASELINE") != "1":
				_check(actor_memory.get("cpu_image_bytes", 0) == 0, "actor drops uploaded CPU images")
				var bindings: Dictionary = child.call("uploaded_texture_bindings")
				for path: String in bindings:
					var rid: RID = bindings[path]
					if texture_rids.has(path):
						repeated_paths += 1
						_check(texture_rids[path] == rid.get_id(), "actors share uploaded " + path)
					texture_rids[path] = rid.get_id()
	if OS.get_environment("SOE_MEMORY_BASELINE") != "1":
		row["actor_textures"] = player.call("npc_texture_memory_attribution")
		if location == "whiterun_gate":
			_check(repeated_paths > 0, "installed actors exercise shared textures")
	row["godot"] = {"video_bytes": Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED), "texture_bytes": Performance.get_monitor(Performance.RENDER_TEXTURE_MEM_USED), "buffer_bytes": Performance.get_monitor(Performance.RENDER_BUFFER_MEM_USED), "objects": Performance.get_monitor(Performance.OBJECT_COUNT), "resources": Performance.get_monitor(Performance.OBJECT_RESOURCE_COUNT), "script_count": null}
	row["pid"] = OS.get_process_id()
	row["uptime_ms"] = Time.get_ticks_msec()
	var status := FileAccess.open("/proc/self/status", FileAccess.READ)
	_check(status != null, "process status opened")
	while status != null and not status.eof_reached():
		var line := status.get_line()
		if line.begins_with("VmRSS:") or line.begins_with("VmHWM:"):
			var fields := line.split(":", true, 1)
			row[fields[0] + "_kib"] = fields[1].strip_edges().split(" ", false)[0].to_int()
	_check(row.get("VmRSS_kib", 0) > 0, "process RSS sampled")
	row["opaque_bytes"] = {"compiled_materials": null, "compiled_shaders": null, "jolt_heap": null, "script_heap": null, "godot_cpu_mesh_copy": null, "godot_cpu_image_copy": null}
	row["scripts"] = player.call("script_memory_attribution")
	if OS.get_environment("SOE_MEMORY_BASELINE") != "1":
		row["player_collision"] = player.call("collision_memory_attribution")
	var scripts := {}
	_add_script(get_script(), scripts)
	var pending: Array[Node] = [root]
	while not pending.is_empty():
		var node: Node = pending.pop_back()
		_add_script(node.get_script(), scripts)
		pending.append_array(node.get_children())
	var source_bytes := 0
	for size: int in scripts.values():
		source_bytes += size
	row.scripts["attached_gdscript_count"] = scripts.size()
	row.scripts["attached_gdscript_source_bytes"] = source_bytes
	row["records"] = streamer.call("plugin_record_memory")
	snapshots.append(row)
	print("MEMORY_ATTRIBUTION ", JSON.stringify(row))
	return row.streamer


func _add_script(script: Script, scripts: Dictionary) -> void:
	while script != null:
		if not scripts.has(script.get_instance_id()):
			scripts[script.get_instance_id()] = script.get_source_code().to_utf8_buffer().size()
		script = script.get_base_script()


func _start() -> void:
	await super._start()
	if not loaded:
		quit(1)
		return
	_configure_player()
	var interior: String = player.call("gameplay_cell")
	var exit: Dictionary = {}
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			exit = door
			break
	_check(not exit.is_empty(), "Bannered Mare exit found")
	if exit.is_empty():
		quit(1)
		return
	var previous := {}
	for cycle in REPEATS:
		var inn := await _snapshot("bannered_mare", cycle)
		_check(player.call("activate_reference", exit.file, exit.object), "inn exit")
		player.call("advance", 0.0)
		var gate: Dictionary = player.call("detection_gate_reference")
		_check(not gate.is_empty(), "Whiterun gate found")
		if gate.is_empty():
			break
		_move(gate)
		await _snapshot("whiterun_gate", cycle)
		_move(exit)
		_check(player.call("gameplay_cell") == interior, "inn return")
		if OS.get_environment("SOE_MEMORY_BASELINE") != "1":
			_check(inn.get("decoded_cache_bytes", 0) <= CACHE_LIMIT, "decoded cache byte bound")
			_check(inn.get("texture_cache_bytes", 0) <= CACHE_LIMIT, "upload cache byte bound")
			_check(inn.get("lod_cache_bytes", 0) <= CACHE_LIMIT, "LOD cache byte bound")
			if cycle >= 2:
				for key in ["decoded_cache_bytes", "texture_cache_bytes", "lod_cache_bytes"]:
					_check(inn.get(key, 0) <= previous.get(key, 0) + STABLE_GROWTH, "stable repeated inn " + key)
				for key in ["mesh_count", "texture_count", "material_count", "shader_count"]:
					_check(inn.get(key, 0) <= previous.get(key, 0), "stable repeated inn " + key)
		previous = inn
	streamer.call("clear")
	await process_frame
	var cleared := await _snapshot("cleared", REPEATS)
	if OS.get_environment("SOE_MEMORY_BASELINE") != "1":
		for key in ["decoded_cache_bytes", "texture_cache_bytes", "lod_cache_bytes"]:
			_check(cleared.get(key, 0) == 0, "clear releases " + key)
	var path := OS.get_environment("SOE_MEMORY_REPORT")
	var file := FileAccess.open(path, FileAccess.WRITE)
	_check(file != null, "attribution report opened")
	if file != null:
		file.store_string(JSON.stringify(snapshots, "\t"))
	print("installed_memory_attribution: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
