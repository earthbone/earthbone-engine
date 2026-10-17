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
	camera.size = 1.0
	camera.position = Vector3(0.5, 2, -0.5)
	camera.rotation.x = -PI / 2
	viewport.add_child(camera)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_DISABLED
	environment.environment.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	viewport.add_child(environment)
	var host := CellStreamerNode.new()
	viewport.add_child(host)
	if not host.load_data(OS.get_environment("SOE_R4_EMISSION_FIXTURE")):
		quit(1)
		return
	for decal in range(2):
		var meshes := host.model_meshes("r4-emission-%d.nif" % decal)
		if meshes.is_empty():
			quit(1)
			return
		var instance := MeshInstance3D.new()
		instance.mesh = meshes[0]
		viewport.add_child(instance)
		for frame in range(5):
			await process_frame
			RenderingServer.force_draw(false)
		var image := viewport.get_texture().get_image()
		var left := image.get_pixel(32, 64).srgb_to_linear().r
		var right := image.get_pixel(96, 64).srgb_to_linear().r
		var expected_left := Color(128.0 / 255.0, 0, 0).srgb_to_linear().r * 0.5
		var expected_right := Color(32.0 / 255.0, 0, 0).srgb_to_linear().r * 0.5
		print("R4 emission decal=", decal, " left=", left, " right=", right,
			" expected=", expected_left, "/", expected_right)
		if absf(left - expected_left) > 0.02 or absf(right - expected_right) > 0.01:
			failures += 1
			printerr("FAIL: emission must retain diffuse texture contrast")
		instance.free()
	host.free()
	viewport.free()
	quit(0 if failures == 0 else 1)
