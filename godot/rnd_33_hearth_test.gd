extends SceneTree

## Captures the installed Bannered Mare fireplace effect through production materials.
const CELL := "WhiterunBanneredMare"
const MODEL := "Clutter/WoodFires/FireplaceWood01Burning.nif"
var failures := 0
var viewport: SubViewport
var camera: Camera3D
var backdrop: MeshInstance3D
var backdrop_material: ShaderMaterial
var instances: Array[MeshInstance3D] = []
var palette_instances: Array[MeshInstance3D] = []

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL rnd_33: ", message)

func _capture() -> Image:
	for _frame in range(4):
		await process_frame
		RenderingServer.force_draw(false)
	return viewport.get_texture().get_image()

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		printerr("FAIL rnd_33: SKYRIM_DATA is not set")
		quit(1)
		return
	var host: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(host)
	_check(host.call("load_data", data), "installed data opens")
	_check(host.call("stream_cell", CELL) > 0, "Bannered Mare hearth cell streams")
	await _capture_room(host)
	var meshes: Array = host.call("model_meshes", MODEL)
	_check(not meshes.is_empty(), "installed fireplace burning model resolves")
	if meshes.is_empty():
		host.free()
		quit(1)
		return
	var bounds := AABB()
	var first := true
	var palette_colors: Array[Vector3] = []
	var has_falloff := false
	for mesh: ArrayMesh in meshes:
		var instance := MeshInstance3D.new()
		instance.mesh = mesh
		viewport = viewport if viewport != null else _make_viewport()
		viewport.add_child(instance)
		instances.append(instance)
		var palette_mesh := ArrayMesh.new()
		bounds = mesh.get_aabb() if first else bounds.merge(mesh.get_aabb())
		first = false
		for surface in range(mesh.get_surface_count()):
			var material := mesh.surface_get_material(surface) as ShaderMaterial
			if material == null:
				continue
			var code: String = material.shader.code
			has_falloff = has_falloff or code.contains("angle_opacity = mix(")
			var maps_palette := code.contains("if (true) { radiance = texture(palette_texture, vec2(texel.g, tint.r))")
			if not maps_palette or not code.contains("blend_add"):
				continue
			if not palette_instances.is_empty() or palette_mesh.get_surface_count() > 0:
				continue
			var palette := material.get_shader_parameter("palette_texture") as Texture2D
			if palette == null:
				continue
			palette_mesh.add_surface_from_arrays(
				mesh.surface_get_primitive_type(surface), mesh.surface_get_arrays(surface)
			)
			palette_mesh.surface_set_material(palette_mesh.get_surface_count() - 1, material)
			var image := palette.get_image()
			if image == null:
				continue
			if image.is_compressed() and image.decompress() != OK:
				continue
			for y in range(image.get_height()):
				for x in range(image.get_width()):
					var color := image.get_pixel(x, y).srgb_to_linear()
					var rgb := Vector3(color.r, color.g, color.b)
					if rgb.length() > 0.02:
						palette_colors.append(rgb)
		if palette_mesh.get_surface_count() > 0:
			instance.mesh = palette_mesh
			palette_instances.append(instance)
		else:
			instance.visible = false
	_check(has_falloff, "hearth material retains authored view falloff")
	_check(not palette_colors.is_empty(), "installed greyscale palette is available")
	_check(not palette_instances.is_empty(), "installed additive palette-color surface is available")
	if palette_colors.is_empty():
		host.free()
		viewport.free()
		quit(1)
		return
	for index in range(palette_instances.size()):
		palette_instances[index].visible = index == 0
	var center := bounds.get_center()
	var extent := maxf(bounds.size.length(), 0.25)
	camera = Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = extent * 1.35
	camera.far = extent * 10.0
	viewport.add_child(camera)
	backdrop = MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = Vector2.ONE * extent * 5.0
	backdrop.mesh = quad
	backdrop_material = ShaderMaterial.new()
	var shader := Shader.new()
	shader.code = "shader_type spatial; render_mode unshaded, cull_disabled; uniform vec4 backdrop_color = vec4(0.0, 0.0, 0.0, 1.0); void fragment() { ALBEDO = backdrop_color.rgb; }"
	backdrop_material.shader = shader
	backdrop.material_override = backdrop_material
	viewport.add_child(backdrop)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color(0.0, 0.0, 0.0, 0.0)
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color.WHITE
	environment.environment.ambient_light_energy = 1.0
	environment.environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
	viewport.add_child(environment)
	var best_pixels := 0
	var best_dark: Image
	var best_light: Image
	var best_empty: Image
	var best_bounds := Rect2()
	var best_direction := Vector3.FORWARD
	for direction in [Vector3.FORWARD, Vector3.BACK, Vector3.RIGHT, Vector3.LEFT, Vector3.UP, Vector3.DOWN]:
		camera.position = center + direction * extent * 2.5
		var camera_up := Vector3.FORWARD if absf(direction.dot(Vector3.UP)) > 0.99 else Vector3.UP
		camera.look_at(center, camera_up)
		backdrop.position = center - direction * extent * 1.5
		backdrop.look_at(camera.position, camera_up)
		backdrop_material.set_shader_parameter("backdrop_color", Color.BLACK)
		var dark := await _capture()
		var visible := 0
		for y in range(dark.get_height()):
			for x in range(dark.get_width()):
				var pixel := dark.get_pixel(x, y).srgb_to_linear()
				if maxf(pixel.r, maxf(pixel.g, pixel.b)) > 0.025:
					visible += 1
		if visible > best_pixels:
			best_pixels = visible
			best_direction = direction
			best_dark = dark
			best_bounds = _project_bounds(bounds, camera)
			backdrop_material.set_shader_parameter("backdrop_color", Color(0.16, 0.20, 0.24))
			best_light = await _capture()
			for instance in instances:
				instance.visible = false
			best_empty = await _capture()
			for instance in instances:
				instance.visible = not palette_instances.is_empty() and instance == palette_instances[0]
	_check(best_pixels >= 20, "fire pixels are visible in the capture: %d" % best_pixels)
	if best_pixels > 0:
		var camera_up := Vector3.FORWARD if absf(best_direction.dot(Vector3.UP)) > 0.99 else Vector3.UP
		camera.position = center + best_direction * extent * 2.5
		camera.look_at(center, camera_up)
		backdrop.position = center - best_direction * extent * 1.5
		backdrop.look_at(camera.position, camera_up)
		var light_pixels := 0
		var bright_sum := 0.0
		var hue_errors := 0
		var transmitted := 0
		var transparent_candidates := 0
		var min_x := maxi(0, int(best_bounds.position.x))
		var min_y := maxi(0, int(best_bounds.position.y))
		var max_x := mini(best_dark.get_width(), int(best_bounds.end.x))
		var max_y := mini(best_dark.get_height(), int(best_bounds.end.y))
		for y in range(min_y, max_y):
			for x in range(min_x, max_x):
				var fire := best_dark.get_pixel(x, y).srgb_to_linear()
				var strength := maxf(fire.r, maxf(fire.g, fire.b))
				if strength > 0.025:
					light_pixels += 1
					bright_sum += maxf(fire.r, maxf(fire.g, fire.b))
					if not _matches_palette(fire, palette_colors):
						hue_errors += 1
						if hue_errors <= 3:
							print("rnd_33 hue-outside-palette rgb=", fire, " distance=", _palette_distance(fire, palette_colors))
				elif best_light != null and best_empty != null:
					transparent_candidates += 1
					var shown := best_light.get_pixel(x, y).srgb_to_linear()
					var clear := best_empty.get_pixel(x, y).srgb_to_linear()
					if Vector3(shown.r, shown.g, shown.b).distance_to(Vector3(clear.r, clear.g, clear.b)) <= 0.05:
						transmitted += 1
		var mean_brightness := bright_sum / maxf(float(light_pixels), 1.0)
		_check(mean_brightness > 0.04, "fire pixels are brighter than the black backdrop: %f" % mean_brightness)
		_check(hue_errors == 0, "fire pixels stay inside the greyscale palette hues: %d of %d" % [hue_errors, light_pixels])
		_check(transparent_candidates > 0 and transmitted == transparent_candidates, "all near-transparent flame pixels transmit the backdrop: %d of %d" % [transmitted, transparent_candidates])
		var output := "res://.godot/rnd_33_bannered_mare_hearth.png"
		_check(best_dark.save_png(output) == OK, "hearth capture saves")
		print("rnd_33_bannered_mare_hearth pixels=", light_pixels, " brightness=", mean_brightness, " out_of_palette=", hue_errors, " transparent_edges=", transmitted, " capture=", output)
	for instance in instances:
		instance.free()
	host.free()
	viewport.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " rnd_33_bannered_mare_hearth_capture")
	quit(0 if failures == 0 else 1)

func _make_viewport() -> SubViewport:
	var result := SubViewport.new()
	result.size = Vector2i(256, 256)
	result.own_world_3d = true
	result.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	get_root().add_child(result)
	return result

func _capture_room(host: Node3D) -> void:
	var bounds: AABB = host.call("scene_bounds")
	var center := bounds.get_center()
	var eye := Vector3(center.x, bounds.position.y + 1.7, center.z)
	var direction := Vector3(-1.0, 0.0, 0.0) if bounds.size.x >= bounds.size.z else Vector3(0.0, 0.0, 1.0)
	var room_camera := Camera3D.new()
	get_root().add_child(room_camera)
	room_camera.position = eye
	room_camera.look_at(eye + direction)
	room_camera.current = true
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color(0.03, 0.035, 0.05)
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color(0.6, 0.6, 0.65)
	environment.environment.ambient_light_energy = 0.6
	get_root().add_child(environment)
	for _frame in range(8):
		await process_frame
		RenderingServer.force_draw(false)
	var image := get_root().get_texture().get_image()
	var scene_pixels := 0
	for y in range(image.get_height()):
		for x in range(image.get_width()):
			var pixel := image.get_pixel(x, y)
			if maxf(pixel.r, maxf(pixel.g, pixel.b)) > 0.08:
				scene_pixels += 1
	_check(scene_pixels > 100, "Bannered Mare hearth scene renders: %d pixels" % scene_pixels)
	_check(image.save_png("res://.godot/rnd_33_bannered_mare_scene.png") == OK, "Bannered Mare scene capture saves")
	print("rnd_33_bannered_mare_scene pixels=", scene_pixels)

func _project_bounds(bounds: AABB, view_camera: Camera3D) -> Rect2:
	var first := true
	var result := Rect2()
	for x in [bounds.position.x, bounds.end.x]:
		for y in [bounds.position.y, bounds.end.y]:
			for z in [bounds.position.z, bounds.end.z]:
				var point := view_camera.unproject_position(Vector3(x, y, z))
				if first:
					result = Rect2(point, Vector2.ZERO)
					first = false
				else:
					result = result.expand(point)
	return result

func _matches_palette(pixel: Color, colors: Array[Vector3]) -> bool:
	return _palette_distance(pixel, colors) <= 0.20

func _palette_distance(pixel: Color, colors: Array[Vector3]) -> float:
	var rgb := Vector3(pixel.r, pixel.g, pixel.b)
	if rgb.length() < 0.0001:
		return 0.0
	rgb /= rgb.x + rgb.y + rgb.z
	var closest := INF
	for palette: Vector3 in colors:
		var sum := palette.x + palette.y + palette.z
		if sum <= 0.0001:
			continue
		var normalized := palette / sum
		closest = minf(closest, rgb.distance_to(normalized))
	return closest
