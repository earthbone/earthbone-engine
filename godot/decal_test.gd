extends SceneTree

## Renders the host's exported shader variants over an opaque surface.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _quad(material: Material, x: float, z: float, width: float) -> MeshInstance3D:
	var instance := MeshInstance3D.new()
	var mesh := QuadMesh.new()
	mesh.size = Vector2(width, 2)
	instance.mesh = mesh
	instance.material_override = material
	instance.position = Vector3(x, 0, z)
	return instance

func _plain(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.albedo_color = color
	return material

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	if DisplayServer.get_name() == "headless":
		printerr("Decal regression requires a renderer")
		quit(1)
		return
	var directory := OS.get_environment("SOE_SHADER_FIXTURES")
	if directory.is_empty():
		printerr("Export host shaders with SOE_SHADER_FIXTURES first")
		quit(1)
		return
	var viewport := SubViewport.new()
	viewport.size = Vector2i(128, 128)
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	viewport.own_world_3d = true
	get_root().add_child(viewport)
	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 2
	camera.position.z = 3
	viewport.add_child(camera)
	var lighting := WorldEnvironment.new()
	lighting.environment = Environment.new()
	lighting.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	lighting.environment.ambient_light_color = Color.WHITE
	lighting.environment.ambient_light_energy = 1
	viewport.add_child(lighting)
	for mode in ["opaque", "cutout", "blend"]:
		for depth_test in [0, 1]:
			for depth_write in [0, 1]:
				var name := "%s-%d-%d" % [mode, depth_test, depth_write]
				var shader := Shader.new()
				shader.code = FileAccess.get_file_as_string(directory.path_join(name + ".gdshader"))
				_check(not shader.code.is_empty(), name + " shader missing")
				var material := ShaderMaterial.new()
				material.shader = shader
				material.render_priority = 1
				var pixels := Image.create(2, 1, false, Image.FORMAT_RGBA8)
				pixels.fill(Color(1, 0, 0, 0.5) if mode == "blend" else Color.RED)
				if mode == "cutout":
					pixels.set_pixel(0, 0, Color(1, 0, 0, 0))
				material.set_shader_parameter("albedo_texture", ImageTexture.create_from_image(pixels))
				for reversed_order in [false, true]:
					var floor_mesh := _quad(_plain(Color.BLUE), 0, 0, 2)
					var decal := _quad(material, 0, 0, 2)
					var cover := _quad(_plain(Color.GREEN), 0.75, 0.5, 0.5)
					var nodes := [decal, floor_mesh, cover] if reversed_order else [floor_mesh, decal, cover]
					for node in nodes:
						viewport.add_child(node)
					await process_frame
					await RenderingServer.frame_post_draw
					var image := viewport.get_texture().get_image()
					var visible := image.get_pixel(80, 64)
					var visible_ok := visible.r > 0.2 and visible.b > 0.2 if mode == "blend" else visible.r > 0.4 and visible.r > visible.b * 2
					_check(visible_ok, name + " hidden by floor: " + str(visible))
					var covered := image.get_pixel(112, 64)
					var covered_ok := covered.g > covered.r * 2 if depth_test == 1 else covered.r > covered.g * 2
					if mode == "blend" and depth_test == 0:
						covered_ok = covered.r > 0.2 and covered.g > 0.2
					_check(covered_ok, name + " depth test: " + str(covered))
					if mode == "cutout":
						var hole := image.get_pixel(16, 64)
						_check(hole.b > hole.r * 2, name + " cutout hole filled: " + str(hole))
					for node in nodes:
						node.free()
	viewport.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " decal variants in both creation orders")
	quit(0 if failures == 0 else 1)
