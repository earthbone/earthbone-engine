extends SceneTree

## Receives the game and host's composed values from synthetic authored IMAD records.
## Pixel checks use a 0.04 linear color change and a 20 percent contrast reduction.

var failures := 0
var viewport: SubViewport
var camera: Camera3D
var material: ShaderMaterial


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


func _variance(image: Image) -> float:
	var total := 0.0
	var squares := 0.0
	var count := 0.0
	for y in range(48, 80):
		for x in range(48, 80):
			var value := image.get_pixel(x, y).get_luminance()
			total += value
			squares += value * value
			count += 1.0
	var mean := total / count
	return squares / count - mean * mean


func _color_difference(left: Color, right: Color) -> float:
	return Vector3(left.r - right.r, left.g - right.g, left.b - right.b).length()


func _apply(values: Dictionary, channel: String) -> void:
	material.set_shader_parameter("tint_color", Color(0.0, 0.0, 0.0, 0.0))
	material.set_shader_parameter("blur_radius", 0.0)
	material.set_shader_parameter("double_vision", 0.0)
	material.set_shader_parameter("bloom_threshold", 0.7)
	material.set_shader_parameter("bloom_scale", 0.0)
	material.set_shader_parameter("saturation", 1.0)
	material.set_shader_parameter("brightness", 1.0)
	material.set_shader_parameter("contrast", 1.0)
	camera.attributes = null
	match channel:
		"color":
			material.set_shader_parameter("tint_color", Color(values.tint[0], values.tint[1], values.tint[2], values.tint[3]))
		"blur":
			material.set_shader_parameter("blur_radius", values.blur_radius)
		"depth":
			var attributes := CameraAttributesPractical.new()
			attributes.dof_blur_far_enabled = values.depth_strength > 0.0
			attributes.dof_blur_amount = values.depth_strength
			attributes.dof_blur_far_distance = values.depth_distance
			attributes.dof_blur_far_transition = values.depth_range
			camera.attributes = attributes
		"hdr":
			material.set_shader_parameter("bloom_threshold", values.bloom_threshold)
			material.set_shader_parameter("bloom_scale", values.bloom_scale)
			material.set_shader_parameter("saturation", values.saturation)
			material.set_shader_parameter("brightness", values.brightness)
			material.set_shader_parameter("contrast", values.contrast)


func _run() -> void:
	if DisplayServer.get_name() == "headless":
		printerr("MAG-71 pixel check needs the configured windowed renderer")
		quit(1)
		return
	var values: Dictionary = JSON.parse_string(OS.get_environment("MAG_71_RENDER_CASE"))
	if values.is_empty():
		printerr("MAG_71_RENDER_CASE is missing")
		quit(1)
		return
	viewport = SubViewport.new()
	viewport.size = Vector2i(128, 128)
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	viewport.own_world_3d = true
	get_root().add_child(viewport)
	camera = Camera3D.new()
	camera.position = Vector3.ZERO
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 4.0
	viewport.add_child(camera)
	camera.make_current()
	var plane := MeshInstance3D.new()
	var mesh := QuadMesh.new()
	mesh.size = Vector2(4.0, 4.0)
	plane.mesh = mesh
	plane.position.z = -18.0
	var source_shader := Shader.new()
	source_shader.code = """
		shader_type spatial;
		render_mode unshaded;
		void fragment() {
			float tile = mod(floor(UV.x * 16.0) + floor(UV.y * 16.0), 2.0);
			ALBEDO = mix(vec3(0.12), vec3(0.48), tile);
		}
	"""
	var source_material := ShaderMaterial.new()
	source_material.shader = source_shader
	plane.material_override = source_material
	viewport.add_child(plane)
	var canvas := CanvasLayer.new()
	viewport.add_child(canvas)
	var overlay := ColorRect.new()
	overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	material = ShaderMaterial.new()
	material.shader = load("res://mag_71_image_space.gdshader")
	overlay.material = material
	canvas.add_child(overlay)

	var baseline := await _pixels()
	var center := baseline.get_pixel(64, 64)
	_apply(values, "color")
	var color_image := await _pixels()
	_check(_color_difference(color_image.get_pixel(64, 64), center) > 0.04,
		"authored animated tint changed the rendered pixel")
	_apply(values, "blur")
	var blur_image := await _pixels()
	_check(_variance(blur_image) < _variance(baseline) * 0.8,
		"authored animated blur reduced checkerboard contrast")
	_apply(values, "depth")
	var depth_image := await _pixels()
	_check(_variance(depth_image) < _variance(baseline) * 0.8,
		"authored animated depth channel blurred the out-of-focus plane")
	_apply(values, "hdr")
	var hdr_image := await _pixels()
	_check(_color_difference(hdr_image.get_pixel(64, 64), center) > 0.04,
		"authored animated HDR curves changed the rendered pixel")

	viewport.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " mag_71 rendered authored IMAD composition")
	quit(0 if failures == 0 else 1)
