extends SceneTree

## RND-39 production falls. Fixed bounds before capture: 100 visible pixels,
## 50 changed pixels, downward optical displacement and 100 translucent pixels.
var failures := 0
var viewport: SubViewport
var camera: Camera3D
var host: CellStreamerNode
var sheets: Array[MeshInstance3D] = []
var materials: Array[ShaderMaterial] = []
var backdrop: MeshInstance3D
var backdrop_material: ShaderMaterial

func _initialize() -> void:
	create_timer(300.0).timeout.connect(func() -> void:
		printerr("FAIL rnd_39: capture timed out")
		quit(1)
	)
	call_deferred("_run")

func _check(ok: bool, label: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL rnd_39: ", label)

func _capture() -> Image:
	for _frame in range(4):
		await process_frame
		RenderingServer.force_draw(false)
	return viewport.get_texture().get_image()

func _time(value: float) -> void:
	for material in materials:
		material.set_shader_parameter("animation_time", value)

func _run() -> void:
	if OS.get_environment("SKYRIM_DATA").is_empty():
		printerr("RND-39 requires installed data")
		quit(1)
		return
	viewport = SubViewport.new()
	viewport.size = Vector2i(256, 256)
	viewport.own_world_3d = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	get_root().add_child(viewport)
	var display := TextureRect.new()
	display.texture = viewport.get_texture()
	display.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	display.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	get_root().add_child(display)
	display.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	camera = Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.far = 10000.0
	viewport.add_child(camera)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color.BLACK
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color.WHITE
	environment.environment.ambient_light_energy = 1.0
	environment.environment.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	environment.environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
	viewport.add_child(environment)
	host = CellStreamerNode.new()
	viewport.add_child(host)
	_check(host.load_data(OS.get_environment("SKYRIM_DATA")), "installed data opens")
	var placements: Array = JSON.parse_string(FileAccess.get_file_as_string("res://.godot/rnd_39/placements.json"))
	var selected: Dictionary = {}
	var resource: ArrayMesh
	for placement: Dictionary in placements:
		if placement.reference != "0:090D19":
			continue
		for mesh: ArrayMesh in host.model_meshes(placement.model):
			var material := mesh.surface_get_material(0) as ShaderMaterial
			if material == null:
				continue
			if material.shader.code.contains("animated_offset.") and material.shader.code.contains("effect_track_0(") and material.shader.code.contains("blend_mix"):
				selected = placement
				resource = mesh
				break
		if resource != null:
			break
	_check(resource != null, "Whiterun has a placed scrolling alpha-blended waterfall")
	if resource == null:
		host.free()
		viewport.free()
		quit(1)
		return
	print("rnd_39 selected ", selected)
	var wrap: Variant = (resource.surface_get_material(0) as ShaderMaterial).get_shader_parameter("wrap_uv")
	_check(typeof(wrap) == TYPE_INT and wrap == 3, "authored wrap axes upload as a packed boolean uniform")
	await _city(selected, resource)
	var sheet := MeshInstance3D.new()
	sheet.mesh = resource
	viewport.add_child(sheet)
	sheets.append(sheet)
	var material := resource.surface_get_material(0) as ShaderMaterial
	materials.append(material)
	var bounds := resource.get_aabb()
	var center := bounds.get_center()
	var extent := maxf(bounds.size.length(), 1.0)
	camera.size = maxf(bounds.size.y, maxf(bounds.size.x, bounds.size.z)) * 1.2
	backdrop = MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = Vector2.ONE * extent * 5.0
	backdrop.mesh = quad
	backdrop_material = ShaderMaterial.new()
	var shader := Shader.new()
	shader.code = "shader_type spatial; render_mode unshaded, cull_disabled; uniform float brightness = 0.0; void fragment() { ALBEDO = vec3(brightness); }"
	backdrop_material.shader = shader
	backdrop.material_override = backdrop_material
	viewport.add_child(backdrop)
	_time(0.25)
	var best_pixels := 0
	var best_direction := Vector3.FORWARD
	for direction in [Vector3.FORWARD, Vector3.BACK, Vector3.LEFT, Vector3.RIGHT]:
		camera.position = center + direction * extent * 2.0
		camera.look_at(center)
		backdrop.position = center - direction * extent
		backdrop.look_at(camera.position)
		var image := await _capture()
		var visible := _visible(image)
		if visible > best_pixels:
			best_pixels = visible
			best_direction = direction
	_check(best_pixels >= 100, "waterfall contributes at least 100 visible pixels: %d" % best_pixels)
	camera.position = center + best_direction * extent * 2.0
	camera.look_at(center)
	backdrop.position = center - best_direction * extent
	backdrop.look_at(camera.position)
	var first := await _capture()
	await create_timer(0.5).timeout
	_time(0.75)
	var second := await _capture()
	var changed := 0
	for y in range(256):
		for x in range(256):
			if absf(first.get_pixel(x, y).r - second.get_pixel(x, y).r) > 0.01:
				changed += 1
	_check(changed >= 50, "authored texture moves over half a second: %d pixels" % changed)
	var best_shift := 0
	var best_error := INF
	for shift in range(-96, 97):
		var error := _motion_error(first, second, shift)
		if error < best_error:
			best_error = error
			best_shift = shift
	# Tiled texture matches can differ by a whole repeat in the isolated view.
	# Direction is asserted in the placed scene against its rock background.
	_time(0.25)
	backdrop_material.set_shader_parameter("brightness", 0.2)
	var light := await _capture()
	sheet.visible = false
	var empty := await _capture()
	sheet.visible = true
	var translucent := 0
	for y in range(256):
		for x in range(256):
			var a := first.get_pixel(x, y).srgb_to_linear().r
			var b := light.get_pixel(x, y).srgb_to_linear().r
			var c := empty.get_pixel(x, y).srgb_to_linear().r
			var transmission := (b - a) / maxf(c, 0.001)
			if a > 0.005 and transmission > 0.05 and transmission < 0.95:
				translucent += 1
	_check(translucent >= 100, "visible falls partly transmit the backdrop: %d pixels" % translucent)
	_check(first.save_png("res://.godot/rnd_39/fall-025.png") == OK, "first capture saves")
	_check(second.save_png("res://.godot/rnd_39/fall-075.png") == OK, "second capture saves")
	_check(light.save_png("res://.godot/rnd_39/fall-translucent.png") == OK, "alpha capture saves")
	print("rnd_39 pixels=", best_pixels, " changed=", changed, " periodic_match_shift=", best_shift, " motion_error=", best_error, " translucent=", translucent)
	_time(-1.0)
	backdrop_material.set_shader_parameter("brightness", 0.0)
	var live_first := await _capture()
	await create_timer(0.5).timeout
	var live_second := await _capture()
	_check(live_first.get_data() != live_second.get_data(), "automatic shader clock animates without material uploads")
	host.free()
	viewport.free()
	print("rnd_39_waterfall_capture: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)

func _visible(image: Image) -> int:
	var count := 0
	for y in range(256):
		for x in range(256):
			if image.get_pixel(x, y).srgb_to_linear().r > 0.005:
				count += 1
	return count

func _motion_error(first: Image, second: Image, shift: int) -> float:
	var total := 0.0
	var count := 0
	for y in range(100, 156, 2):
		for x in range(80, 176, 2):
			var a := first.get_pixel(x, y).r
			if a < 0.05:
				continue
			var b := second.get_pixel(x, y + shift).r
			total += absf(a - b)
			count += 1
	return total / count if count >= 100 else INF

func _city(placement: Dictionary, resource: ArrayMesh) -> void:
	_check(host.open_cell_id(placement.cell) > 0, "Whiterun waterfall cell streams through production rendering")
	var bounds := resource.get_aabb()
	# This fixed reference has zero rotation and unit scale. Frame the selected
	# sheet rather than the survey's first animated surface, which is its skirt.
	# units.rs supplies metres per unit and the (x, z, -y) axis conversion.
	var origin := Vector3(float(placement.position[0]), float(placement.position[2]), -float(placement.position[1])) * 0.0142875
	var center := origin + bounds.get_center()
	var material := resource.surface_get_material(0) as ShaderMaterial
	var original_color: Vector4 = material.get_shader_parameter("base_color")
	var original_scale: float = material.get_shader_parameter("base_scale")
	material.set_shader_parameter("animation_time", 0.25)
	camera.size = maxf(bounds.size.length() * 1.2, 3.0)
	var best_visible := 0
	var best_direction := Vector3.FORWARD
	for direction in [Vector3.FORWARD, Vector3.BACK, Vector3.LEFT, Vector3.RIGHT]:
		camera.position = center + direction * camera.size * 2.0
		camera.look_at(center)
		material.set_shader_parameter("base_scale", original_scale)
		var drawn := await _capture()
		material.set_shader_parameter("base_scale", 0.0)
		var black_water := await _capture()
		var visible := 0
		for y in range(256):
			for x in range(256):
				if drawn.get_pixel(x, y).r - black_water.get_pixel(x, y).r > 0.01:
					visible += 1
		if visible > best_visible:
			best_visible = visible
			best_direction = direction
	_check(best_visible >= 100, "placed Whiterun waterfall contributes visible pixels: %d" % best_visible)
	camera.position = center + best_direction * camera.size * 2.0
	camera.look_at(center)
	material.set_shader_parameter("base_scale", original_scale)
	var first := await _capture()
	await create_timer(0.5).timeout
	material.set_shader_parameter("animation_time", 0.75)
	var second := await _capture()
	material.set_shader_parameter("base_scale", 0.0)
	var second_attenuated := await _capture()
	material.set_shader_parameter("animation_time", 0.25)
	var attenuated := await _capture()
	var first_water := _contribution(first, attenuated)
	var second_water := _contribution(second, second_attenuated)
	var best_shift := 0
	var best_error := INF
	for shift in range(-80, 81):
		var error := _city_motion_error(first_water, second_water, shift)
		if error < best_error:
			best_shift = shift
			best_error = error
	_check(best_shift > 0, "placed Whiterun texture moves downward against its rocks: %d pixels" % best_shift)
	material.set_shader_parameter("base_color", Vector4(original_color.x, original_color.y, original_color.z, 0.0))
	var rocks := await _capture()
	var translucent := 0
	for y in range(256):
		for x in range(256):
			var full := first.get_pixel(x, y).srgb_to_linear().r
			var black := attenuated.get_pixel(x, y).srgb_to_linear().r
			var rock := rocks.get_pixel(x, y).srgb_to_linear().r
			if rock > 0.02 and full - black > 0.005:
				var transmission := black / rock
				if transmission > 0.05 and transmission < 0.95:
					translucent += 1
	_check(translucent >= 100, "visible falls partly transmit actual Whiterun rocks: %d pixels" % translucent)
	material.set_shader_parameter("base_color", original_color)
	material.set_shader_parameter("base_scale", original_scale)
	material.set_shader_parameter("animation_time", -1.0)
	_check(first.save_png("res://.godot/rnd_39/whiterun-first.png") == OK, "Whiterun scene first capture saves")
	_check(second.save_png("res://.godot/rnd_39/whiterun-second.png") == OK, "Whiterun scene second capture saves")
	_check(rocks.save_png("res://.godot/rnd_39/whiterun-rocks.png") == OK, "Whiterun rocks control saves")
	_check(first.get_data() != second.get_data(), "placed waterfall animates against the Whiterun rocks")
	print("rnd_39 Whiterun visible=", best_visible, " rock_translucency=", translucent, " down_shift=", best_shift, " motion_error=", best_error)
	host.clear()

func _contribution(drawn: Image, attenuated: Image) -> Image:
	var water := Image.create(256, 256, false, Image.FORMAT_RF)
	for y in range(256):
		for x in range(256):
			var full := drawn.get_pixel(x, y).srgb_to_linear().r
			var black := attenuated.get_pixel(x, y).srgb_to_linear().r
			water.set_pixel(x, y, Color(maxf(full - black, 0.0), 0.0, 0.0))
	return water

func _city_motion_error(first: Image, second: Image, shift: int) -> float:
	var total := 0.0
	var count := 0
	for y in range(80, 176, 2):
		for x in range(0, 256, 2):
			var a := first.get_pixel(x, y).r
			if a < 0.005:
				continue
			total += absf(a - second.get_pixel(x, y + shift).r)
			count += 1
	# Every sample represents four screen pixels, matching the 100 pixel bound.
	return total / count if count >= 25 else INF
