extends SceneTree

var failures := 0
var viewport: SubViewport

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _capture() -> Image:
	for frame in range(5):
		await process_frame
		RenderingServer.force_draw(false)
	return viewport.get_texture().get_image()

func _run() -> void:
	viewport = SubViewport.new()
	viewport.size = Vector2i(256, 256)
	viewport.own_world_3d = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	viewport.positional_shadow_atlas_size = 1024
	root.add_child(viewport)
	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 3.0
	camera.position.z = 4.0
	viewport.add_child(camera)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color.BLACK
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_DISABLED
	environment.environment.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	viewport.add_child(environment)
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(0.6, 0.6, 0.6)
	material.roughness = 1.0
	var receiver := MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = Vector2(3.0, 3.0)
	receiver.mesh = quad
	receiver.material_override = material
	viewport.add_child(receiver)
	var blocker := MeshInstance3D.new()
	var box := BoxMesh.new()
	box.size = Vector3(0.3, 0.3, 0.3)
	blocker.mesh = box
	blocker.material_override = material
	blocker.position = Vector3(0.5, 0.4, 0.7)
	viewport.add_child(blocker)
	var host := CellStreamerNode.new()
	viewport.add_child(host)
	_check(host.load_data(OS.get_environment("SOE_R4_LIGHT_FIXTURE")), "fixture loads")
	_check(host.stream_cell("R4Lights") >= 0, "record-only cell streams")
	host.set_process(false)
	var lights: Array = host.call("record_light_instances") if host.has_method("record_light_instances") else []
	print("R4 record light count=", lights.size())
	_check(lights.size() == 3, "three enabled placed lights, including model-less LIGH references")
	if lights.size() != 3:
		host.free()
		viewport.free()
		quit(1)
		return
	_check(lights.filter(func(row: Dictionary) -> bool: return row.shadow).size() == 2, "only the two authored shadow types cast shadows")
	_check(lights.filter(func(row: Dictionary) -> bool: return row.spot).size() == 1, "one spotlight")
	for row: Dictionary in lights:
		RenderingServer.instance_set_visible(row.instance, false)
	for row: Dictionary in lights:
		if not row.shadow:
			continue
		RenderingServer.instance_set_visible(row.instance, true)
		var shadowed := await _capture()
		RenderingServer.light_set_shadow(row.light, false)
		var unshadowed := await _capture()
		var colored := 0
		var shadow_pixels := 0
		for y in range(256):
			for x in range(256):
				var a := shadowed.get_pixel(x, y).srgb_to_linear()
				var b := unshadowed.get_pixel(x, y).srgb_to_linear()
				if b.r > 0.03 and b.r > b.g * 1.2:
					colored += 1
				if b.r - a.r > 0.02:
					shadow_pixels += 1
		print("R4 record light spot=", row.spot, " colored=", colored, " shadow_pixels=", shadow_pixels)
		_check(colored >= 100, "record color and radius illuminate the receiver")
		_check(shadow_pixels >= 20, "authored shadow flag produces a visible blocker shadow")
		RenderingServer.light_set_shadow(row.light, true)
		RenderingServer.instance_set_visible(row.instance, false)
	host.set_process(true)
	var energies := {}
	for frame in range(60):
		await process_frame
		for row: Dictionary in host.call("record_light_instances"):
			if not row.shadow or row.spot:
				continue
			_check(row.energy >= 1.5 and row.energy <= 2.5, "flicker stays within authored amplitude")
			energies[snappedf(row.energy, 0.001)] = true
	_check(energies.size() > 1, "record flicker animates")
	print("R4 record flicker distinct_energies=", energies.size())
	host.clear()
	_check(host.call("record_light_instances").is_empty(), "clear releases placed light resources")
	host.free()
	viewport.free()
	quit(0 if failures == 0 else 1)
