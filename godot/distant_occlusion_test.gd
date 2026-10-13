# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("OCCLUSION_FAIL ", message)

func _box(position: Vector3, size: Vector3, color: Color) -> void:
	var instance := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	mesh.size = size
	instance.mesh = mesh
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.albedo_color = color
	instance.material_override = material
	instance.position = position
	root.add_child(instance)

func _run() -> void:
	_box(Vector3(0, 1, 0), Vector3(8, 2, 0.2), Color.GRAY)
	_box(Vector3(-1, 1.5, -3), Vector3(1, 3, 1), Color.RED)
	_box(Vector3(1, 0.5, -3), Vector3(0.5, 0.5, 0.5), Color.BLUE)
	var camera := Camera3D.new()
	root.add_child(camera)
	camera.position = Vector3(0, 1, 10)
	camera.look_at(Vector3(0, 1, 0))
	camera.make_current()
	var vertices := PackedVector3Array([Vector3(-3.9, 0.1, -0.09), Vector3(3.9, 0.1, -0.09), Vector3(3.9, 1.9, -0.09), Vector3(-3.9, 1.9, -0.09)])
	var mesh := RenderingServer.occluder_create()
	RenderingServer.occluder_set_mesh(mesh, vertices, PackedInt32Array([0, 1, 2, 0, 2, 3]))
	var instance := RenderingServer.instance_create2(mesh, root.world_3d.scenario)
	RenderingServer.instance_set_transform(instance, Transform3D.IDENTITY)
	var calls := []
	var pixels := []
	for enabled: bool in [false, true]:
		root.use_occlusion_culling = enabled
		for _frame in range(36):
			await process_frame
		await RenderingServer.frame_post_draw
		calls.append(int(Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME)))
		var image := root.get_texture().get_image()
		var red := 0
		for y in range(image.get_height()):
			for x in range(image.get_width()):
				var color := image.get_pixel(x, y)
				red += int(color.r > 0.9 and color.g < 0.1 and color.b < 0.1)
		pixels.append(red)
	_check(calls[1] < calls[0], "fully hidden AABB is culled")
	_check(pixels[0] > 100 and absi(pixels[1] - pixels[0]) <= 5, "partly visible AABB keeps its exposed top")
	RenderingServer.free_rid(instance)
	RenderingServer.free_rid(mesh)
	print("OCCLUSION_RESULT ", "PASS" if failures == 0 else "FAIL", " draw_calls=", calls, " exposed_pixels=", pixels)
	quit(0 if failures == 0 else 1)
