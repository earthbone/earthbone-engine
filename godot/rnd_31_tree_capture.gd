extends SceneTree

const DETAIL_PIXELS := 500
const BACKGROUND := Color(0.04, 0.08, 0.14)
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var target_values := OS.get_environment("SOE_RND31_TARGET").split(",")
	var capture := OS.get_environment("SOE_RND31_CAPTURE_PATH")
	var origin := OS.get_environment("SOE_RND31_ORIGIN")
	var radius := int(OS.get_environment("SOE_RND31_RADIUS"))
	if data.is_empty() or target_values.size() != 3 or capture.is_empty() or origin.is_empty() or DisplayServer.get_name() == "headless":
		printerr("FAIL: RND-31 requires installed data, a target and a windowed renderer")
		quit(2)
		return
	var target := Vector3(float(target_values[0]), float(target_values[2]), -float(target_values[1])) * 0.0142875
	var viewport := SubViewport.new()
	viewport.size = Vector2i(640, 360)
	viewport.own_world_3d = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	var camera := Camera3D.new()
	camera.position = target + Vector3(0.0, 3.2, 9.0)
	camera.fov = 48.0
	viewport.add_child(camera)
	camera.look_at(target + Vector3(0.0, 2.5, 0.0), Vector3.UP)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = BACKGROUND
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color(0.72, 0.78, 0.68)
	environment.environment.ambient_light_energy = 1.0
	viewport.add_child(environment)
	var host := CellStreamerNode.new()
	viewport.add_child(host)
	_check(host.load_data(data), "installed data load")
	_check(host.stream_exterior(origin, radius) > 0, "Whiterun exterior tree cell streams")
	for _frame in range(8):
		await process_frame
		RenderingServer.force_draw(false)
	var image := viewport.get_texture().get_image()
	var detail_pixels := 0
	var colors := {}
	var background_pixels := 0
	var checker_pixels := 0
	for y in range(0, mini(320, image.get_height())):
		for x in range(180, mini(460, image.get_width())):
			var color := image.get_pixel(x, y)
			var background_distance := absf(color.r - BACKGROUND.r) + absf(color.g - BACKGROUND.g) + absf(color.b - BACKGROUND.b)
			if background_distance < 0.035:
				background_pixels += 1
			else:
				detail_pixels += 1
				colors[color.to_rgba32()] = true
			if color.r > 0.55 and color.b > 0.55 and color.g < 0.2:
				checker_pixels += 1
	_check(detail_pixels >= DETAIL_PIXELS, "tree has no visible texture detail in the capture")
	_check(colors.size() >= 32, "tree texture lacks varied leaf and bark pixels")
	_check(background_pixels >= 300, "alpha-tested leaf gaps do not show the scene background")
	_check(checker_pixels <= 4, "tree crop shows the magenta missing-texture checker")
	var saved := image.save_png(capture)
	_check(saved == OK, "capture image saves")
	print("RND-31 capture: detail_pixels=", detail_pixels, " colors=", colors.size(), " background_pixels=", background_pixels, " checker_pixels=", checker_pixels)
	host.free()
	viewport.free()
	if failures == 0:
		print("RND-31 RESULT: PASS")
	else:
		printerr("RND-31 RESULT: FAIL: ", failures)
	quit(0 if failures == 0 else 1)
