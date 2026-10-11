extends SceneTree
## EDT-22: NPC records generate the uploaded vertices and a changed rendered silhouette.

var instances: Array[RID] = []

func _initialize() -> void:
	create_timer(15.0).timeout.connect(func(): _fail("render check timed out"))
	_run.call_deferred()

func _fail(message: String) -> void:
	push_error("EDT_22: " + message)
	for instance in instances:
		RenderingServer.free_rid(instance)
	instances.clear()
	quit(1)

func _run() -> void:
	var directory := OS.get_environment("EDT_22_FIXTURE")
	if directory.is_empty():
		_fail("EDT_22_FIXTURE is unset")
		return
	var table := SoeFormTable.new()
	var paths := PackedStringArray([
		directory.path_join("Unrelated.esm"),
		directory.path_join("Base.esm"),
		directory.path_join("Patch.esp"),
	])
	if not table.open(paths, "Patch.esp"):
		_fail(table.last_error())
		return
	var meshes := table.generate_npc_head("1:000900", directory)
	if meshes.size() != 2:
		_fail("expected head and extra: " + table.last_error())
		return
	var expected := PackedVector3Array([
		Vector3(2.5, 5.0, 0.0) * 0.0142875,
		Vector3(105.0, 5.0, 0.0) * 0.0142875,
		Vector3(2.5, 10.0, -100.0) * 0.0142875,
	])
	var arrays: Array = meshes[0].surface_get_arrays(0)
	var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	if vertices.size() != expected.size():
		_fail("uploaded vertex count differs")
		return
	for index in expected.size():
		if not vertices[index].is_equal_approx(expected[index]):
			_fail("uploaded NAM9 vertex differs at " + str(index))
			return
	if arrays[Mesh.ARRAY_INDEX] != PackedInt32Array([0, 2, 1]):
		_fail("uploaded topology differs")
		return
	var world := Node3D.new()
	root.add_child(world)
	var camera := Camera3D.new()
	world.add_child(camera)
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 2.4
	camera.position = Vector3(0.7, 3.0, -0.7)
	camera.look_at(Vector3(0.7, 0.0, -0.7), Vector3(0.0, 0.0, -1.0))
	camera.current = true
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color.BLACK
	world.add_child(environment)
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	material.albedo_color = Color.WHITE
	for mesh in meshes:
		mesh.surface_set_material(0, material)
		var instance := RenderingServer.instance_create2(mesh.get_rid(), world.get_world_3d().scenario)
		instances.append(instance)
	var expected_arrays: Array = []
	expected_arrays.resize(Mesh.ARRAY_MAX)
	expected_arrays[Mesh.ARRAY_VERTEX] = expected
	expected_arrays[Mesh.ARRAY_INDEX] = PackedInt32Array([0, 2, 1])
	var expected_mesh := ArrayMesh.new()
	expected_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, expected_arrays)
	expected_mesh.surface_set_material(0, material)
	instances.append(RenderingServer.instance_create2(expected_mesh.get_rid(), world.get_world_3d().scenario))
	RenderingServer.instance_set_visible(instances[2], false)
	RenderingServer.instance_set_visible(instances[0], false)
	await process_frame
	await RenderingServer.frame_post_draw
	var base := root.get_texture().get_image()
	RenderingServer.instance_set_visible(instances[1], false)
	RenderingServer.instance_set_visible(instances[0], true)
	await process_frame
	await RenderingServer.frame_post_draw
	var generated := root.get_texture().get_image()
	RenderingServer.instance_set_visible(instances[0], false)
	RenderingServer.instance_set_visible(instances[2], true)
	await process_frame
	await RenderingServer.frame_post_draw
	var reference := root.get_texture().get_image()
	var lit := 0
	var changed := 0
	var mismatched := 0
	for y in generated.get_height():
		for x in generated.get_width():
			var current := generated.get_pixel(x, y).r > 0.5
			if current:
				lit += 1
			if current != (base.get_pixel(x, y).r > 0.5):
				changed += 1
			if current != (reference.get_pixel(x, y).r > 0.5):
				mismatched += 1
	if lit < 3000 or changed < 100 or mismatched > 8:
		_fail("render thresholds failed: lit=%d changed=%d mismatched=%d" % [lit, changed, mismatched])
		return
	if generated.save_png(directory.path_join("edt_22_face.png")) != OK:
		_fail("screenshot write failed")
		return
	for instance in instances:
		RenderingServer.free_rid(instance)
	instances.clear()
	print("EDT_22_RENDER_OK lit=%d changed=%d mismatched=%d" % [lit, changed, mismatched])
	quit(0)
