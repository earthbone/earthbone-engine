extends "res://walkthrough.gd"

var checking := false
var failures := 0

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _process(_delta: float) -> bool:
	if loaded and not checking:
		checking = true
		call_deferred("_check_fire")
	return false

func _check_fire() -> void:
	player.call("advance", 1.0 / 60.0)
	var count: int = streamer.call("record_light_instances").size() if streamer.has_method("record_light_instances") else 0
	print("R4 Bannered Mare record lights=", count)
	if count == 0:
		failures += 1
		printerr("FAIL: placed hearth and room LIGH references must render")
	if not streamer.has_method("nif_particle_count") or int(streamer.call("nif_particle_count")) == 0:
		failures += 1
		printerr("FAIL: the central campfire requires live flame and smoke particles")
	player.set_process(false)
	player.set_physics_process(false)
	var old_camera := root.get_camera_3d()
	var camera := Camera3D.new()
	camera.environment = old_camera.environment
	camera.attributes = old_camera.attributes
	root.add_child(camera)
	camera.make_current()
	var bounds := AABB()
	var first := true
	for row: Dictionary in streamer.call("nif_surfaces"):
		if not row.models.has("clutter/woodfires/campfire01burning.nif"):
			continue
		print("R4 hearth layer source=", row.source, " effect=", row.effect,
			" refraction=", row.refraction, " flags=", row.flags1, "/", row.flags2,
			" blend=", row.src_blend, "/", row.dst_blend, " bounds=", row.bounds)
		var material := row.material as ShaderMaterial
		if material != null:
			for parameter in ["source_texture", "distortion_texture", "palette_texture", "base_color", "base_scale", "falloff"]:
				print("R4 ", parameter, "=", material.get_shader_parameter(parameter))
		if row.effect and str(row.source).contains("glowslightflash") and (int(row.flags1) & 16) != 0:
			failures += 1
			printerr("FAIL: the central radial glow has an inferred palette")
		if row.effect:
			bounds = row.bounds if first else bounds.merge(row.bounds)
			first = false
	for row: Dictionary in streamer.call("nif_surfaces"):
		if not ["candle", "window", "torch", "brazier"].any(func(word: String) -> bool: return str(row.source).to_lower().contains(word)):
			continue
		print("R4 lit object source=", row.source, " models=", row.models,
			" flags=", row.flags1, "/", row.flags2, " bounds=", row.bounds)
		if row.material is StandardMaterial3D:
			var material := row.material as StandardMaterial3D
			print("R4 standard emission=", material.emission_enabled, " color=", material.emission,
				" strength=", material.emission_energy_multiplier, " glow=", material.emission_texture)
		elif row.material is ShaderMaterial:
			var material := row.material as ShaderMaterial
			print("R4 shader emission=", material.get_shader_parameter("emission_color"),
				" strength=", material.get_shader_parameter("emission_strength"),
				" glow=", material.get_shader_parameter("glow_texture"))
	var extent := maxf(bounds.size.length(), 0.5)
	var center := bounds.get_center()
	camera.position = center + Vector3(0.0, 0.8, -1.2) * extent * 0.65
	camera.look_at(center)
	for frame in range(8):
		await process_frame
		RenderingServer.force_draw(false)
	var image := root.get_texture().get_image()
	var output := OS.get_environment("SOE_R4_OUTPUT")
	if output.is_empty():
		output = "res://../target"
	if image.save_png(output + "/r4_fire_%s.png" % OS.get_environment("SOE_R4_PHASE")) != OK:
		failures += 1
		printerr("FAIL: hearth capture could not be saved")
	var warm := 0
	var white := 0
	for y in range(image.get_height() / 4, image.get_height() * 3 / 4):
		for x in range(image.get_width() / 4, image.get_width() * 3 / 4):
			var c := image.get_pixel(x, y)
			if c.r > 0.4 and c.r > c.b * 1.8 and c.g > c.b * 1.2:
				warm += 1
			if c.r > 0.8 and c.g > 0.8 and c.b > 0.8:
				white += 1
	print("R4 hearth warm_pixels=", warm, " white_pixels=", white)
	if warm < 100 or white > image.get_width() * image.get_height() / 100:
		failures += 1
		printerr("FAIL: hearth requires visible warm flames and less than one percent white coverage")
	# CONTEXT: Readback and pixel analysis must leave the frame timing sample.
	var settle_until := Time.get_ticks_msec() + 2000
	while Time.get_ticks_msec() < settle_until:
		await process_frame
	var cpu_ms: Array[float] = []
	var frame_ms: Array[float] = []
	var previous := Time.get_ticks_usec()
	for sample in range(120):
		await process_frame
		var now := Time.get_ticks_usec()
		frame_ms.append((now - previous) / 1000.0)
		cpu_ms.append(Performance.get_monitor(Performance.TIME_PROCESS) * 1000.0)
		previous = now
	cpu_ms.sort()
	frame_ms.sort()
	var device := RenderingServer.get_rendering_device()
	print("R4 Bannered Mare process_p99_ms=", cpu_ms[118], " frame_p99_ms=", frame_ms[118],
		" existing_frame_budget_ms=16.7 renderer=", device.get_device_name() if device != null else "Compatibility")
	if frame_ms[118] > 16.7:
		failures += 1
		printerr("FAIL: Bannered Mare exceeds the existing 16.7 ms p99 frame budget")
	quit(0 if failures == 0 else 1)
