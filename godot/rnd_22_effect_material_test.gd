extends SceneTree

## Production effect resources. Bounds precede the first capture run.
## Synthetic channel error <= 0.025 linear. Vanilla visible pixels >= 100,
## p90 radiance < 0.8, mean < 0.45 and p10 backdrop transmission > 0.02.
var failures := 0
var viewport: SubViewport
var camera: Camera3D
var host: CellStreamerNode
var backdrop: MeshInstance3D
var backdrop_material: ShaderMaterial

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _capture() -> Image:
	for frame in range(4):
		await process_frame
		RenderingServer.force_draw(false)
	return viewport.get_texture().get_image()

func _backdrop(value: float) -> void:
	backdrop_material.set_shader_parameter("brightness", value)

func _run() -> void:
	if DisplayServer.get_name() == "headless":
		printerr("RND-22 needs a renderer")
		quit(1)
		return
	viewport = SubViewport.new()
	viewport.size = Vector2i(128, 128)
	viewport.own_world_3d = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	get_root().add_child(viewport)
	camera = Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 1.0
	camera.position = Vector3(0, 0, 2)
	camera.far = 10000.0
	viewport.add_child(camera)
	var lighting := WorldEnvironment.new()
	lighting.environment = Environment.new()
	lighting.environment.background_mode = Environment.BG_COLOR
	lighting.environment.background_color = Color.BLACK
	lighting.environment.ambient_light_source = Environment.AMBIENT_SOURCE_DISABLED
	lighting.environment.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	lighting.environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
	viewport.add_child(lighting)
	backdrop = MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = Vector2(4, 4)
	backdrop.mesh = quad
	backdrop.position.z = -1
	backdrop_material = ShaderMaterial.new()
	var shader := Shader.new()
	shader.code = "shader_type spatial; render_mode unshaded, cull_disabled; uniform float brightness = 0.05; void fragment() { ALBEDO = vec3(brightness); }"
	backdrop_material.shader = shader
	backdrop.material_override = backdrop_material
	viewport.add_child(backdrop)
	host = CellStreamerNode.new()
	viewport.add_child(host)
	if not OS.get_cmdline_user_args().has("--vanilla-only"):
		_check(host.load_data(OS.get_environment("SOE_RND22_FIXTURE")), "synthetic data opens")
		_backdrop(0.05)
		var expected := [0.1375, 0.15, 0.45, 0.4375, 0.02, 0.09375, 0.068]
		for i in range(14):
			var resources := host.model_meshes("effect%d.nif" % i)
			_check(resources.size() == 1, "fixture %d produces one surface" % i)
			if resources.is_empty():
				continue
			var instance := MeshInstance3D.new()
			instance.mesh = resources[0]
			viewport.add_child(instance)
			if i == 8:
				backdrop.position.z = -0.01
			else:
				backdrop.position.z = -1
			var pixels := await _capture()
			var actual := pixels.get_pixel(64, 64).srgb_to_linear()
			if i < 7:
				_check(absf(actual.r - expected[i]) <= 0.025, "blend/falloff/vertices %d: %s expected=%f" % [i, actual, expected[i]])
			elif i == 7:
				var palette := Color(128.0/255.0, 64.0/255.0, 32.0/255.0).srgb_to_linear()
				for channel in range(3):
					var value: float = palette[channel] * (128.0/255.0) + 0.05 * (127.0/255.0)
					_check(absf(actual[channel] - value) <= 0.025, "palette channel %d: %s expected=%f" % [channel, actual, value])
			elif i in [8, 11]:
				_check(absf(actual.r - 0.05) <= 0.01, "soft depth or alpha test %d: %s" % [i, actual])
			elif i == 9:
				_check(absf(actual.r - 0.1375) <= 0.025, "unsoftened control remains visible")
			elif i == 10:
				_check(absf(actual.r - 0.4) <= 0.025, "opaque effect writes its authored radiance")
			elif i in [12, 13]:
				var color := Color(128.0/255.0, 64.0/255.0, 32.0/255.0).srgb_to_linear() if i == 12 else Color(1.0, 64.0/255.0, 128.0/255.0).srgb_to_linear()
				var opacity := (192.0/255.0) * 0.25 if i == 12 else 128.0/255.0
				for channel in range(3):
					var value: float = color[channel] * (1.0 if i == 12 else 0.4) * opacity + 0.05 * (1.0 - opacity)
					_check(absf(actual[channel] - value) <= 0.025, "palette flag %d channel %d actual=%f expected=%f" % [i, channel, actual[channel], value])
			print("rnd_22_synthetic ", i, " linear=", actual)
			instance.free()
		_backdrop(0.05)
		backdrop.position.z = -1
		if not OS.get_cmdline_user_args().has("--skip-blends"):
			for src in range(11):
				for dst in range(11):
					var resources := host.model_meshes("blend%d_%d.nif" % [src, dst])
					_check(resources.size() == 1, "blend fixture exists")
					if resources.is_empty():
						continue
					var instance := MeshInstance3D.new()
					instance.mesh = resources[0]
					viewport.add_child(instance)
					var image := await _capture()
					var actual := image.get_pixel(64, 64).srgb_to_linear()
					var value := 0.4 * _factor(src) + 0.05 * _factor(dst)
					_check(absf(actual.r - value) <= 0.025, "blend %d,%d actual=%f expected=%f" % [src, dst, actual.r, value])
					instance.free()
			print("rnd_22_all_blend_factors_over_opaque_background: 121 pairs")
	host.free()
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("SKYRIM_DATA unset, vanilla captures skipped")
	else:
		host = CellStreamerNode.new()
		viewport.add_child(host)
		_check(host.load_data(data), "installed data opens")
		lighting.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
		lighting.environment.ambient_light_color = Color.WHITE
		lighting.environment.ambient_light_energy = 1.0
		await _vanilla("effects/ambient/fxmistlow01halfvis.nif", "mist")
		await _vanilla("architecture/windhelm/effects/whfxwindowglow01.nif", "window")
		host.free()
	viewport.free()
	print("rnd_22_effect_material_capture: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)

func _vanilla(model: String, label: String) -> void:
	var instances: Array[MeshInstance3D] = []
	var bounds := AABB()
	var first := true
	for resource in host.model_meshes(model):
		var material := resource.surface_get_material(0) as ShaderMaterial
		if material == null or not material.shader.code.contains("depth_draw_never"):
			continue
		var instance := MeshInstance3D.new()
		instance.mesh = resource
		viewport.add_child(instance)
		instances.append(instance)
		bounds = resource.get_aabb() if first else bounds.merge(resource.get_aabb())
		first = false
	_check(not instances.is_empty(), label + " has blended effect surfaces")
	if instances.is_empty():
		return
	var center := bounds.get_center()
	var extent := maxf(bounds.size.length(), 1.0)
	camera.size = extent * 0.85
	var quad := backdrop.mesh as QuadMesh
	quad.size = Vector2.ONE * extent * 4
	var best_visible := 0
	var best_brightness: Array[float] = []
	var best_transmission: Array[float] = []
	var best_image: Image
	for direction in [Vector3(0, 0.5, 1).normalized(), Vector3(1, 0.5, 0).normalized(), Vector3(0, 1, 0.2).normalized()]:
		camera.position = center + direction * extent * 2
		camera.look_at(center)
		backdrop.position = center - direction * extent * 2
		backdrop.look_at(camera.position)
		_backdrop(0.0)
		var dark := await _capture()
		_backdrop(0.2)
		var bright := await _capture()
		var brightness: Array[float] = []
		var transmission: Array[float] = []
		for y in range(128):
			for x in range(128):
				var a := dark.get_pixel(x, y).srgb_to_linear()
				var b := bright.get_pixel(x, y).srgb_to_linear()
				var signal_strength := maxf(a.r, maxf(a.g, a.b))
				if signal_strength > 0.003:
					brightness.append(signal_strength)
					transmission.append(((b.r - a.r) + (b.g - a.g) + (b.b - a.b)) / 0.6)
		if brightness.size() > best_visible:
			best_visible = brightness.size()
			best_brightness = brightness
			best_transmission = transmission
			best_image = dark
	_check(best_visible >= 100, label + " visible pixels: " + str(best_visible))
	if best_visible > 0:
		best_brightness.sort()
		best_transmission.sort()
		var p90 := best_brightness[int(best_visible * 0.9)]
		var mean := 0.0
		for value in best_brightness:
			mean += value / best_visible
		var p10 := best_transmission[int(best_visible * 0.1)]
		_check(p90 < 0.8 and mean < 0.45, label + " brightness p90=%f mean=%f" % [p90, mean])
		_check(p10 > 0.02, label + " backdrop transmission p10=" + str(p10))
		var output := "res://.godot/rnd_22-%s.png" % label
		_check(best_image.save_png(output) == OK, "capture saved")
		print("rnd_22_vanilla ", label, " pixels=", best_visible, " p90=", p90, " mean=", mean, " transmission_p10=", p10, " capture=", output)
	for instance in instances:
		instance.free()

func _factor(kind: int) -> float:
	match kind:
		0: return 1.0
		1: return 0.0
		2: return 0.4
		3: return 0.6
		4: return 0.05
		5: return 0.95
		6: return 0.25
		7: return 0.75
		8: return 1.0
		9: return 0.0
		_: return 0.0
