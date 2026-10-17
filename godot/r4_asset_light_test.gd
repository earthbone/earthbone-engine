extends SceneTree

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	var viewport := SubViewport.new()
	viewport.size = Vector2i(256, 256)
	viewport.own_world_3d = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	viewport.add_child(camera)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color(0.03, 0.03, 0.03)
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color.WHITE
	environment.environment.ambient_light_energy = 0.1
	environment.environment.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	viewport.add_child(environment)
	var host := CellStreamerNode.new()
	viewport.add_child(host)
	if not host.load_data(OS.get_environment("SOE_R4_EMISSION_FIXTURE")):
		quit(1)
		return
	for model in ["clutter/woodfires/campfire01burning.nif", "clutter/candles/candlehornwall01.nif", "clutter/candles/candlehornchandelier01.nif", "architecture/whiterun/wrinteriors/wrintwallstr01wina.nif"]:
		var meshes := host.model_meshes(model)
		if model.contains("wrintwall"):
			var emitting: Array[ArrayMesh] = []
			for candidate: ArrayMesh in meshes:
				var material := candidate.surface_get_material(0)
				var standard := material as StandardMaterial3D
				var shader := material as ShaderMaterial
				if standard != null and standard.emission_enabled:
					emitting.append(candidate)
				elif shader != null and shader.get_shader_parameter("emission_strength") != null and float(shader.get_shader_parameter("emission_strength")) > 0.0:
					emitting.append(candidate)
			meshes = emitting
		if meshes.is_empty():
			failures += 1
			continue
		if model.contains("campfire"):
			var radial := meshes[1].surface_get_material(0) as ShaderMaterial
			var paletted := radial != null and radial.get_shader_parameter("palette_texture") != null
			print("R4 radial glow inferred_palette=", paletted)
			if paletted:
				failures += 1
				printerr("FAIL: authored radial glow has an inferred palette")
		var group := Node3D.new()
		viewport.add_child(group)
		var bounds: AABB = meshes[0].get_aabb()
		for mesh in meshes:
			var instance := MeshInstance3D.new()
			instance.mesh = mesh
			group.add_child(instance)
			bounds = bounds.merge(mesh.get_aabb())
		var center := bounds.get_center()
		var span := maxf(bounds.size.x, maxf(bounds.size.y, bounds.size.z))
		camera.size = span * 1.4
		camera.position = center + Vector3(1.0, 0.5, 1.5) * span
		if model.contains("wrintwall"):
			var normal: Vector3 = meshes[0].surface_get_arrays(0)[Mesh.ARRAY_NORMAL][0]
			camera.position = center + normal * span * 1.5
		camera.look_at(center)
		for frame in range(5):
			await process_frame
			RenderingServer.force_draw(false)
		var image := viewport.get_texture().get_image()
		var clipped := 0
		var warm := 0
		var visible := 0
		for y in range(256):
			for x in range(256):
				var color := image.get_pixel(x, y)
				if maxf(color.r, maxf(color.g, color.b)) > 0.1:
					visible += 1
				if minf(color.r, minf(color.g, color.b)) > 0.98:
					clipped += 1
				if color.r > 0.4 and color.r > color.b * 1.5:
					warm += 1
		print("R4 asset ", model, " clipped=", clipped, " warm=", warm, " visible=", visible)
		var output := OS.get_environment("SOE_R4_OUTPUT")
		if output.is_empty():
			output = "res://../target"
		if image.save_png(output + "/r4_%s_%s.png" % [model.get_file().get_basename(), OS.get_environment("SOE_R4_PHASE")]) != OK:
			failures += 1
			printerr("FAIL: asset capture could not be saved")
		if visible < 100:
			failures += 1
			printerr("FAIL: fewer than 100 visible object pixels")
		if clipped > 655:
			failures += 1
			printerr("FAIL: more than one percent of the object view is clipped white")
		group.free()
	host.free()
	viewport.free()
	quit(0 if failures == 0 else 1)
