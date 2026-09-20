extends SceneTree

## Production materials, DDS faces and texture caches under an unlit viewport.
## Limits declared before running: 0.08 sRGB error and 0.03 for black samples.
var failures := 0
const DIRECTIONS := [Vector3.RIGHT, Vector3.LEFT, Vector3.FORWARD, Vector3.BACK, Vector3.UP, Vector3.DOWN]
const FACE_COLORS := [Color.RED, Color.GREEN, Color.BLUE, Color.YELLOW, Color.MAGENTA, Color.CYAN]

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _compare(actual: Color, expected_linear: Color, name: String) -> void:
	var expected := expected_linear.linear_to_srgb()
	for channel in range(3):
		var tolerance := 0.03 if expected[channel] == 0.0 else 0.08
		_check(absf(actual[channel] - expected[channel]) < tolerance, name + " actual=" + str(actual) + " expected=" + str(expected))

func _run() -> void:
	var directory := OS.get_environment("SOE_ENVIRONMENT_FIXTURE")
	if directory.is_empty() or DisplayServer.get_name() == "headless":
		printerr("Environment regression needs SOE_ENVIRONMENT_FIXTURE and a renderer")
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
		var order := range(14)
		if reversed_order:
			order.reverse()
		for i in order:
			var face: int = i if i < 6 else 4
			camera.position = DIRECTIONS[face] * 2
			camera.look_at(Vector3.ZERO, Vector3.FORWARD if face >= 4 else Vector3.UP)
			host.clear()
			_check(host.stream_cell("Environment%d" % i) == 1, "one fixture surface")
			for frame in range(3):
				await process_frame
				RenderingServer.force_draw(false)
			var pixels := viewport.get_texture().get_image()
			var left := pixels.get_pixel(32, 64)
			var right := pixels.get_pixel(96, 64)
			var strength := 0.5 if i in [7, 8, 12] else 0.25
			if i in [9, 10]:
				strength = 0.0
			if i == 12:
				strength *= 128.0 / 255.0
			var color: Color = Color.YELLOW if i == 11 else FACE_COLORS[face]
			var expected := Color(color.r * strength, color.g * strength, color.b * strength)
			_compare(left, expected, "Environment%d left" % i)
			_compare(right, Color.BLACK if i in [6, 7, 8, 10, 13] else expected, "Environment%d right" % i)
			print("ENVIRONMENT cell=", i, " reverse=", reversed_order, " left=", left, " right=", right)
		host.free()
	viewport.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " cubemap directions, masks and scale in both cache orders")
	quit(0 if failures == 0 else 1)
