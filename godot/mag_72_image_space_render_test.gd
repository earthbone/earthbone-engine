extends SceneTree

## Pixel readback for the stock Godot controls used by the MAG-72 host mapping.
## The game-side fixture resolves CELL/XCIM and WTHR/IMSP before these values
## are applied here.

var failures := 0
var viewport: SubViewport
var camera: Camera3D
var environment: Environment
var overlay: ColorRect


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func _pixels() -> Image:
	for frame in 3:
		await process_frame
		RenderingServer.force_draw(false)
	return viewport.get_texture().get_image()


func _image_difference(left: Image, right: Image) -> float:
	var total := 0.0
	for y in viewport.size.y:
		for x in viewport.size.x:
			total += _color_difference(left.get_pixel(x, y), right.get_pixel(x, y))
	return total / float(viewport.size.x * viewport.size.y)


func _color_difference(left: Color, right: Color) -> float:
	return Vector4(left.r - right.r, left.g - right.g, left.b - right.b, left.a - right.a).length()


func _apply(values: Dictionary) -> void:
	environment.adjustment_enabled = true
	environment.adjustment_saturation = values.saturation
	environment.adjustment_brightness = values.brightness
	environment.adjustment_contrast = values.contrast
	environment.tonemap_exposure = values.exposure
	environment.tonemap_white = values.white
	environment.glow_enabled = values.bloom > 0.0
	environment.glow_intensity = values.bloom
	camera.environment = environment
	var attributes := CameraAttributesPractical.new()
	attributes.dof_blur_far_enabled = values.depth_amount > 0.0
	attributes.dof_blur_amount = values.depth_amount
	attributes.dof_blur_far_distance = values.depth_distance
	attributes.dof_blur_far_transition = values.depth_range
	camera.attributes = attributes
	overlay.color = Color(values.tint[1], values.tint[2], values.tint[3], values.tint[0])
	overlay.visible = values.tint[0] > 0.0


func _run() -> void:
	if DisplayServer.get_name() == "headless":
		printerr("MAG-72 render check requires a windowed renderer")
		quit(1)
		return
	var cases: Dictionary = JSON.parse_string(OS.get_environment("MAG_72_RENDER_CASES"))
	if cases.is_empty():
		printerr("MAG_72_RENDER_CASES is missing")
		quit(1)
		return
	viewport = SubViewport.new()
	viewport.size = Vector2i(160, 90)
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	viewport.own_world_3d = true
	get_root().add_child(viewport)
	camera = Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 4.0
	viewport.add_child(camera)
	camera.make_current()
	var plane := MeshInstance3D.new()
	var plane_mesh := QuadMesh.new()
	plane_mesh.size = Vector2(4.0, 2.25)
	plane.mesh = plane_mesh
	plane.position = Vector3(0.0, 0.0, -18.0)
	var checker := ShaderMaterial.new()
	checker.shader = Shader.new()
	checker.shader.code = """
		shader_type spatial;
		render_mode unshaded;
		void fragment() {
			float cell = mod(floor(UV.x * 24.0) + floor(UV.y * 14.0), 2.0);
			ALBEDO = mix(vec3(0.05), vec3(0.95), cell);
		}
	"""
	plane.material_override = checker
	viewport.add_child(plane)
	environment = Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.2, 0.25, 0.3)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_DISABLED
	environment.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
	var world_environment := WorldEnvironment.new()
	world_environment.environment = environment
	viewport.add_child(world_environment)
	var canvas := CanvasLayer.new()
	viewport.add_child(canvas)
	overlay = ColorRect.new()
	overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	overlay.visible = false
	canvas.add_child(overlay)

	var clear_image := await _pixels()
	var clear_pixel := clear_image.get_pixel(80, 45)
	var cell: Dictionary = cases["cell"]
	_apply(cell)
	var cell_image := await _pixels()
	var cell_pixel := cell_image.get_pixel(80, 45)
	_check(_color_difference(cell_pixel, clear_pixel) > 0.08,
		"CELL/XCIM HDR, color and tint inputs did not change rendered pixels")
	_check(environment.tonemap_exposure == cell.exposure and environment.tonemap_white == cell.white,
		"CELL/XCIM HDR controls were not applied")
	_check(camera.attributes.dof_blur_far_enabled and
		is_equal_approx(camera.attributes.dof_blur_amount, cell.depth_amount) and
		is_equal_approx(camera.attributes.dof_blur_far_distance, cell.depth_distance) and
		is_equal_approx(camera.attributes.dof_blur_far_transition, cell.depth_range),
		"CELL/XCIM depth controls were not applied")
	var cell_depth_image := cell_image
	camera.attributes = null
	var cell_sharp_image := await _pixels()
	_apply(cell)
	cell_depth_image = await _pixels()
	var depth_difference := _image_difference(cell_depth_image, cell_sharp_image)
	print("MAG-72 depth pixel difference=", depth_difference)
	_check(depth_difference > 0.0001,
		"CELL/XCIM depth-of-field did not change rendered pixels")

	var weather: Dictionary = cases["weather"]
	_apply(weather)
	var weather_image := await _pixels()
	var weather_pixel := weather_image.get_pixel(80, 45)
	_check(_color_difference(weather_pixel, cell_pixel) > 0.08,
		"selected WTHR/IMSP inputs did not change rendered pixels")
	_check(environment.tonemap_exposure == weather.exposure and environment.tonemap_white == weather.white,
		"WTHR/IMSP HDR controls were not applied")
	_check(camera.attributes.dof_blur_far_enabled and
		is_equal_approx(camera.attributes.dof_blur_amount, weather.depth_amount) and
		is_equal_approx(camera.attributes.dof_blur_far_distance, weather.depth_distance) and
		is_equal_approx(camera.attributes.dof_blur_far_transition, weather.depth_range),
		"WTHR/IMSP depth controls were not applied")

	_apply(cases["restored"])
	var restored_image := await _pixels()
	_check(_color_difference(restored_image.get_pixel(80, 45), cell_pixel) < 0.02,
		"clearing selected weather did not restore CELL/XCIM pixels")
	_check(failures == 0, "MAG-72 rendered fixture failed")
	print("MAG-72 pixels clear=", clear_pixel, " cell=", cell_pixel,
		" weather=", weather_pixel, " restored=", restored_image.get_pixel(80, 45))
	print("RESULT: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
