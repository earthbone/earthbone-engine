# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Inspect the same submitted resources used in free walk.
var failures := 0
var camera: Camera3D
var saved_materials: Array = []

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _process(_delta: float) -> bool:
	return false

func _level_at(stats: Dictionary, cell: Vector2i) -> int:
	for text: String in String(stats.get("blocks", "")).split(" ", false):
		var parts := text.split(":")
		var span := int(parts[0])
		var origin := Vector2i(int(parts[1]), int(parts[2]))
		if cell.x >= origin.x and cell.x < origin.x + span and cell.y >= origin.y and cell.y < origin.y + span:
			return span
	return 2147483647

func _texture_info(texture: Texture2D) -> String:
	return "missing" if texture == null else "%dx%d" % [texture.get_width(), texture.get_height()]

func _shader_diffuse(material: ShaderMaterial) -> Texture2D:
	var uniform := "albedo_texture" if material.shader.code.contains("uniform sampler2D albedo_texture") else "diffuse_texture"
	return material.get_shader_parameter(uniform)

func _material_info(material: Material) -> Dictionary:
	if material is BaseMaterial3D:
		return {"diffuse": _texture_info(material.albedo_texture), "normal": _texture_info(material.normal_texture), "normal_enabled": material.normal_enabled}
	if material is ShaderMaterial:
		return {"diffuse": _texture_info(_shader_diffuse(material)), "normal": _texture_info(material.get_shader_parameter("normal_texture")), "normal_uses_mips": material.shader.code.contains("texture(normal_texture"), "has_normal": material.get_shader_parameter("has_normal")}
	return {}

func _rss_mib(key: String) -> float:
	var file := FileAccess.open("/proc/self/status", FileAccess.READ)
	if file == null:
		return INF
	while not file.eof_reached():
		var line := file.get_line()
		if line.begins_with(key + ":"):
			return float(line.get_slice(":", 1).strip_edges().get_slice(" ", 0)) / 1024.0
	return INF

func _band_detail(rows: Array, cell: Vector2i) -> Vector2i:
	var detail := Vector2i.ZERO
	for row: Dictionary in rows:
		if row.kind != "object":
			continue
		var span := int(row.level)
		if cell.x < int(row.x) or cell.x >= int(row.x) + span or cell.y < int(row.y) or cell.y >= int(row.y) + span:
			continue
		detail.x += int(row.triangles)
		var material: Material = row.mesh.surface_get_material(0)
		if material is BaseMaterial3D and material.albedo_texture != null:
			detail.y = maxi(detail.y, material.albedo_texture.get_width())
		elif material is ShaderMaterial:
			var diffuse := _shader_diffuse(material)
			if diffuse != null:
				detail.y = maxi(detail.y, diffuse.get_width())
	return detail

func _capture(label: String) -> void:
	for _frame in range(6):
		await process_frame
	await RenderingServer.frame_post_draw
	var error := root.get_texture().get_image().save_png("res://.godot/mountain-" + label + ".png")
	if error != OK:
		failures += 1

func _start() -> void:
	await super._start()
	if not loaded:
		quit(1)
		return
	system_overlay.free()
	for overlay in root.find_children("*", "CanvasLayer", true, false):
		overlay.hide()
	var exited := false
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			exited = player.call("activate_reference", door.file, door.object)
			break
	if not exited:
		quit(1)
		return
	player.call("advance", 0.0)
	while player.call("is_load_door_pending") or player.call("is_location_preparing"):
		player.call("advance", 1.0 / 60.0)
		await process_frame
	for _frame in range(90):
		player.call("advance", 1.0 / 60.0)
		await process_frame
	var feet: Vector3 = player.call("position")
	camera = Camera3D.new()
	root.add_child(camera)
	camera.position = Vector3(feet.x, feet.z + 1000.0, -feet.y) * 0.0142875
	camera.far = 12000.0
	camera.fov = 85.0
	camera.look_at(camera.position + Vector3(0.0, 0.18, 1.0) * 1000.0)
	camera.make_current()
	print("MOUNTAIN_CAMERA ", camera.position, " feet=", feet)
	print("MOUNTAIN_TERRAIN ", streamer.call("terrain_lod_stats"))
	print("MOUNTAIN_OBJECT ", streamer.call("object_lod_stats"))
	var object_stats: Dictionary = streamer.call("object_lod_stats")
	var medium := _level_at(object_stats, Vector2i(12, -10))
	var far_band := _level_at(object_stats, Vector2i(12, -18))
	print("MOUNTAIN_BANDS medium=", medium, " far=", far_band)
	if medium > far_band or medium != 4:
		failures += 1
		printerr("MOUNTAIN_FAIL nearer mountain lacks its authored level 4 object detail")
	for heading in [0, 90, 180, 270]:
		var angle := deg_to_rad(float(heading))
		camera.look_at(camera.position + Vector3(sin(angle), 0.18, cos(angle)) * 1000.0)
		await _capture("normal-" + str(heading))
	camera.look_at(camera.position + Vector3(0.0, 0.18, 1.0) * 1000.0)
	root.use_occlusion_culling = false
	await _capture("occlusion-off")
	var rows: Array = streamer.call("play_path_surfaces")
	if streamer.has_method("lod_band_surfaces"):
		rows = rows.filter(func(row: Dictionary) -> bool: return row.kind != "lod")
		rows.append_array(streamer.call("lod_band_surfaces", ""))
	else:
		for mesh: ArrayMesh in streamer.call("terrain_lod_meshes"):
			rows.append({"mesh": mesh, "bounds": mesh.get_aabb(), "kind": "btr"})
	if streamer.has_method("lod_band_surfaces"):
		var scenery: Array = streamer.call("lod_band_surfaces", "object")
		var near_detail := _band_detail(scenery, Vector2i(12, -10))
		var far_detail := _band_detail(scenery, Vector2i(12, -18))
		print("MOUNTAIN_DETAIL medium=", near_detail, " far=", far_detail)
		if near_detail.x == 0 or far_detail.x == 0 or near_detail.y == 0 or far_detail.y == 0 or near_detail.y < far_detail.y:
			failures += 1
			printerr("MOUNTAIN_FAIL nearer mountain has coarser geometry or texture")
	for row: Dictionary in rows:
		var mesh: ArrayMesh = row.mesh
		var bounds: AABB = row.bounds
		if bounds.size.y > 60.0 and bounds.end.z > camera.position.z:
			print("MOUNTAIN_SURFACE ", JSON.stringify({"kind": row.kind, "bounds": str(bounds), "triangles": mesh.surface_get_array_index_len(0) / 3, "references": row.get("references", []), "material": _material_info(mesh.surface_get_material(0)), "level": row.get("level", 0), "source": row.get("source", ""), "diffuse": row.get("diffuse", ""), "normal": row.get("normal", "")}))
		var material := StandardMaterial3D.new()
		material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		if row.has("level"):
			var colors := {4: Color.GREEN, 8: Color.YELLOW, 16: Color.RED, 32: Color.BLUE}
			material.albedo_color = colors[int(row.level)]
			if row.kind == "object":
				material.albedo_color = material.albedo_color.lerp(Color.CYAN, 0.6)
		else:
			material.albedo_color = Color.MAGENTA if row.kind == "btr" else (Color.RED if row.kind == "lod" else Color.LIGHT_GRAY)
		saved_materials.append([mesh, mesh.surface_get_material(0)])
		mesh.surface_set_material(0, material)
	await _capture("representations")
	for saved: Array in saved_materials:
		saved[0].surface_set_material(0, saved[1])
	var worst_update_ms := 0.0
	var focus := Vector2i(floori(feet.x / 4096.0), floori(feet.y / 4096.0))
	for _sample in range(20):
		var started := Time.get_ticks_usec()
		streamer.call("update_object_lod", focus.x, focus.y)
		worst_update_ms = maxf(worst_update_ms, (Time.get_ticks_usec() - started) / 1000.0)
		await process_frame
	var peak_mib := _rss_mib("VmHWM")
	print("MOUNTAIN_BUDGET peak_mib=", peak_mib, " limit_mib=6144 worst_update_ms=", worst_update_ms, " limit_ms=33")
	if peak_mib > 6144.0 or worst_update_ms > 33.0:
		failures += 1
		printerr("MOUNTAIN_FAIL existing memory or update budget exceeded")
	print("MOUNTAIN_RESULT ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
