extends SceneTree

## Pixel fixtures for the modifier shader used by PlayerNode.
## Limits: each selected channel must change a sample by at least 0.04 linear.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _pixels(viewport: SubViewport) -> Image:
	for frame in range(3):
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

func _run() -> void:
	if DisplayServer.get_name() == "headless":
		printerr("mag_71 rendered fixture needs the configured windowed display")
		quit(1)
		return
	var viewport := SubViewport.new()
	viewport.size = Vector2i(128, 128)
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	viewport.own_world_3d = true
	get_root().add_child(viewport)
	var camera := Camera3D.new()
	camera.position = Vector3.ZERO
	viewport.add_child(camera)
	var light := DirectionalLight3D.new()
	light.rotation = Vector3(-0.6, 0.0, 0.0)
	viewport.add_child(light)
	var quad := MeshInstance3D.new()
	var mesh := QuadMesh.new()
	mesh.size = Vector2(4.0, 4.0)
	quad.mesh = mesh
	quad.position.z = -3.0
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
	quad.material_override = source_material
	viewport.add_child(quad)
	var image_pass := ColorRect.new()
	image_pass.position = Vector2.ZERO
	image_pass.size = Vector2(128.0, 128.0)
	image_pass.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var shader_material := ShaderMaterial.new()
	shader_material.shader = load("res://mag_71_image_space.gdshader")
	image_pass.material = shader_material
	var layer := CanvasLayer.new()
	viewport.add_child(layer)
	layer.add_child(image_pass)
	var baseline := await _pixels(viewport)
	var baseline_variance := _variance(baseline)

	shader_material.set_shader_parameter("tint_color", Color(1.0, 0.0, 0.0, 0.8))
	var color_frame := await _pixels(viewport)
	var color_pixel := color_frame.get_pixel(64, 64)
	_check(color_pixel.r > color_pixel.g + 0.2, "authored tint changed rendered pixel color")
	shader_material.set_shader_parameter("tint_color", Color(0.0, 0.0, 0.0, 0.0))

	shader_material.set_shader_parameter("blur_radius", 3.0)
	var blur_frame := await _pixels(viewport)
	_check(_variance(blur_frame) < baseline_variance * 0.8,
		"authored blur reduced checkerboard contrast")
	shader_material.set_shader_parameter("blur_radius", 0.0)

	var attributes := CameraAttributesPractical.new()
	attributes.dof_blur_far_enabled = true
	attributes.dof_blur_amount = 1.0
	attributes.dof_blur_far_distance = 0.1
	attributes.dof_blur_far_transition = 0.1
	camera.attributes = attributes
	var depth_frame := await _pixels(viewport)
	_check(_variance(depth_frame) < baseline_variance * 0.8,
		"authored depth curve blurred the out-of-focus plane")
	camera.attributes = null

	shader_material.set_shader_parameter("exposure", 1.8)
	var hdr_frame := await _pixels(viewport)
	_check(hdr_frame.get_pixel(64, 64).get_luminance() > baseline.get_pixel(64, 64).get_luminance() + 0.04,
		"authored HDR exposure changed rendered luminance")

	viewport.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " mag_71 rendered IMAD color blur depth and HDR")
	quit(0 if failures == 0 else 1)
