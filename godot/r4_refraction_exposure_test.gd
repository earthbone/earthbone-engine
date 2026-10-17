extends SceneTree

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	var viewport := SubViewport.new()
	viewport.size = Vector2i(128, 128)
	viewport.own_world_3d = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 2.0
	camera.position.z = 2.0
	viewport.add_child(camera)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_DISABLED
	environment.environment.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	viewport.add_child(environment)
	var background := MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = Vector2(4, 4)
	background.mesh = quad
	var material := ShaderMaterial.new()
	material.shader = Shader.new()
	material.shader.code = "shader_type spatial; render_mode unshaded, fog_disabled; void fragment() { ALBEDO = vec3(0.1, 0.15, 0.2); }"
	background.material_override = material
	viewport.add_child(background)
	var hull := MeshInstance3D.new()
	quad = QuadMesh.new()
	quad.size = Vector2.ONE
	hull.mesh = quad
	hull.position.z = 0.1
	material = ShaderMaterial.new()
	material.shader = Shader.new()
	material.shader.code = FileAccess.get_file_as_string("res://../crates/godot-host/src/nif_refraction.gdshader").replace("$CULL", "cull_disabled").replace("$DEPTH_TEST", "").replace("$VERTEX_ALPHA", "1.0")
	hull.material_override = material
	viewport.add_child(hull)
	var attributes := CameraAttributesPractical.new()
	camera.attributes = attributes
	for exposure in [1.0, 2.0, 4.0]:
		for multiplier in [1.0, 2.0, 4.0]:
			environment.environment.tonemap_exposure = exposure
			attributes.exposure_multiplier = multiplier
			for frame in range(4):
				await process_frame
				RenderingServer.force_draw(false)
			var image := viewport.get_texture().get_image()
			var through := image.get_pixel(64, 64)
			var outside := image.get_pixel(16, 16)
			var error := Vector3(through.r - outside.r, through.g - outside.g, through.b - outside.b).abs().length()
			print("R4 refraction exposure=", exposure, " multiplier=", multiplier,
				" through=", through, " outside=", outside, " error=", error)
			if error > 0.02:
				failures += 1
				printerr("FAIL: heat hull changes flat-background brightness")
	viewport.free()
	quit(0 if failures == 0 else 1)
