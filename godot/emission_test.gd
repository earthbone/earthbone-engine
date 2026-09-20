extends SceneTree

## Render independently authored NIF emission through the production cell path.
## Predeclared limits: linear-to-sRGB brightness within 0.08, dark below 0.03.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var directory := OS.get_environment("SOE_EMISSION_FIXTURE")
	if directory.is_empty() or DisplayServer.get_name() == "headless":
		printerr("Emission regression needs SOE_EMISSION_FIXTURE and a renderer")
		quit(1)
		return
	var viewport := SubViewport.new()
	viewport.size = Vector2i(128, 128)
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	viewport.own_world_3d = true
	get_root().add_child(viewport)
	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 1.0
	camera.position = Vector3(0.5, 2, -0.5)
	camera.rotation.x = -PI / 2
	viewport.add_child(camera)
	var lighting := WorldEnvironment.new()
	lighting.environment = Environment.new()
	lighting.environment.background_mode = Environment.BG_COLOR
	lighting.environment.background_color = Color.BLACK
	lighting.environment.ambient_light_source = Environment.AMBIENT_SOURCE_DISABLED
	lighting.environment.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	lighting.environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
	viewport.add_child(lighting)
	for reversed_order in [false, true]:
		var host := CellStreamerNode.new()
		viewport.add_child(host)
		if not host.load_data(directory):
			_check(false, "fixture did not load")
			host.free()
			break
		var order := range(8)
		if reversed_order:
			order.reverse()
		for i in order:
			host.clear()
			_check(host.stream_cell("Emission%d" % i) == 1, "one fixture surface")
			_check(host.last_stats().materials == 1, "distinct material cache entry")
			for frame in range(3):
				await process_frame
				RenderingServer.force_draw(false)
			var pixels := viewport.get_texture().get_image()
			var left := pixels.get_pixel(32, 64)
			var right := pixels.get_pixel(96, 64)
			var variant: int = i % 4
			var strength: float = [0.0, 0.25, 0.5, 0.25][variant]
			var expected := Color(strength, 0, 0).linear_to_srgb().r
			_check(absf(left.r - expected) < 0.08, "Emission%d authored strength: %s expected %s" % [i, left, expected])
			_check(left.g < 0.03 and left.b < 0.03, "authored red emission color")
			if variant == 3:
				_check(absf(right.r - expected) < 0.08, "slot 2 without glow flag must not mask color-only emission")
			else:
				_check(right.r < 0.03 and right.g < 0.03 and right.b < 0.03, "unlit control or black glow texel: " + str(right))
			print("EMISSION cell=", i, " reverse=", reversed_order, " left=", left, " right=", right)
		host.free()
	viewport.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " emission standard/decal in both cache orders")
	quit(0 if failures == 0 else 1)
