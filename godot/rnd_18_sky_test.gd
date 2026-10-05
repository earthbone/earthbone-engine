extends SceneTree

const RecordSky := preload("res://record_sky.gd")
const COLOR_TOLERANCE := 0.08
const REQUIRED_SKY_PIXELS := 500
const REQUIRED_CLOUD_MOTION_PIXELS := 100
var failed := false

func _initialize() -> void:
	call_deferred("_run")

func check(value: bool, message: String) -> void:
	if not value:
		failed = true
		printerr("RND-18 FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("RND-18 SKIP: SKYRIM_DATA unset")
		quit()
		return
	var scene := Node3D.new()
	root.add_child(scene)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	scene.add_child(streamer)
	if not streamer.call("load_data", data):
		check(false, "load data")
		quit(1)
		return
	check(streamer.call("stream_exterior", "WhiterunOrigin", 0) > 0, "render Whiterun")
	var bounds: AABB = streamer.call("scene_bounds")
	var camera := Camera3D.new()
	scene.add_child(camera)
	camera.position = bounds.get_center() + Vector3(0.0, bounds.size.y * 0.5 + 8.0, 60.0)
	camera.rotation_degrees = Vector3(35.0, 30.0, 0.0)
	camera.far = 2000.0
	camera.make_current()
	var sky := RecordSky.new()
	scene.add_child(sky)
	sky.set_process(false)
	check(sky.use_streamer(streamer, "WhiterunOrigin", 12.0), "initialize sky")
	check(sky.records.set_weather("SkyrimClear", true), "select vanilla SkyrimClear")
	check(sky.refresh(), "read clear weather")
	var screenshot_dir := "res://../target/rnd_18"
	DirAccess.make_dir_recursive_absolute(screenshot_dir)
	var sky_brightness: Array[float] = []
	for hour in [12.0, 0.0]:
		check(sky.records.set_time(hour, 0.0), "set game time")
		check(sky.refresh(), "refresh current WTHR")
		for _frame in range(8):
			await process_frame
		await RenderingServer.frame_post_draw
		var image := root.get_texture().get_image()
		check(image.save_png(screenshot_dir + ("/noon.png" if hour == 12.0 else "/night.png")) == OK, "save capture")
		var expected: Color = sky.last_frame.upper
		var matched := 0
		var brightness := 0.0
		for y in range(10, 100):
			for x in range(10, 630):
				var pixel := image.get_pixel(x, y)
				brightness += pixel.v
				if maxf(absf(pixel.r - expected.r), maxf(absf(pixel.g - expected.g), absf(pixel.b - expected.b))) <= COLOR_TOLERANCE:
					matched += 1
		check(matched >= REQUIRED_SKY_PIXELS, "sky pixels match NAM0 at hour %s: %s" % [hour, matched])
		sky_brightness.append(brightness / (90.0 * 620.0))
		print("RND-18 capture hour=", hour, " expected=", expected, " matches=", matched, " mean_brightness=", sky_brightness.back(), " weather=", sky.last_frame.weather)
		check(sky.sun.visible == (hour == 12.0), "sun light follows the sun direction")
		check(is_equal_approx(sky.environment.fog_depth_begin, sky.last_frame.fog.x / 70.0), "fog near reaches Environment")
		check(is_equal_approx(sky.environment.fog_depth_end, sky.last_frame.fog.y / 70.0), "fog far reaches Environment")
		check(sky.fog_material.get_shader_parameter("far_color") == sky.last_frame.fog_far_color, "far fog color reaches the depth pass")
		check(is_equal_approx(sky.fog_material.get_shader_parameter("fog_max"), clampf(sky.last_frame.fog.w, 0.0, 1.0)), "maximum fog opacity reaches the depth pass")
		check(sky.material.get_shader_parameter("cloud_count") == sky.last_frame.clouds.size(), "all cloud layers reach shader")
		var speeds: PackedVector2Array = sky.material.get_shader_parameter("cloud_speeds")
		for index in sky.last_frame.clouds.size():
			check(speeds[index] == sky.last_frame.clouds[index].speed, "authored cloud velocity reaches shader")
		for key in ["sun_texture", "masser_texture", "secunda_texture"]:
			check(sky.material.get_shader_parameter(key) != null, key + " resolves through VFS")
		check(sky.material.get_shader_parameter("stars_enabled"), "CLMT night model texture resolves")
	check(sky_brightness[0] > sky_brightness[1] + 0.05, "night sky region is darker than noon")
	check(sky.records.set_time(12.0, 0.0) and sky.refresh(), "restore noon for cloud motion capture")
	for _frame in range(8):
		await process_frame
	await RenderingServer.frame_post_draw
	var before_motion := root.get_texture().get_image()
	sky.elapsed += 120.0
	check(sky.refresh(), "advance cloud presentation time")
	for _frame in range(8):
		await process_frame
	await RenderingServer.frame_post_draw
	var after_motion := root.get_texture().get_image()
	var changed := 0
	for y in range(10, 100):
		for x in range(10, 630):
			var a := before_motion.get_pixel(x, y)
			var b := after_motion.get_pixel(x, y)
			if maxf(absf(a.r - b.r), maxf(absf(a.g - b.g), absf(a.b - b.b))) > 0.02:
				changed += 1
	check(changed >= REQUIRED_CLOUD_MOTION_PIXELS, "authored cloud speeds move rendered sky pixels: %s" % changed)
	print("RND-18 cloud motion pixels=", changed)
	check(sky.records.set_time(0.0, sky.last_frame.phase_days * 4.0), "new moon time")
	check(sky.refresh() and sky.last_frame.phase == 4, "CLMT phase length selects new moons")
	check(sky.records.set_weather("SkyrimCloudy", false), "request cloudy transition")
	check(sky.refresh() and sky.last_frame.transition == 0.0, "transition begins at the old weather")
	sky.records.advance(1.0)
	check(sky.refresh() and sky.last_frame.transition > 0.0 and sky.last_frame.transition < 1.0, "WTHR transition advances in game hours")
	check(sky.use_streamer(streamer, "WhiterunBanneredMare", 12.0), "interior lighting")
	check(sky.last_frame.has("rotation"), "interior retains its own directional lighting")
	check(sky.environment.ambient_light_color == sky.last_frame.ambient, "interior ambient template reaches Environment")
	var fog_end: float = sky.last_frame.fog.y
	if sky.last_frame.fog_clip > 0.0:
		fog_end = minf(fog_end, sky.last_frame.fog_clip)
	check(is_equal_approx(sky.fog_material.get_shader_parameter("fog_far"), fog_end / 70.0), "interior fog template reaches depth pass")
	for index in sky.ambient_lights.size():
		check(sky.ambient_lights[index].visible and sky.ambient_lights[index].light_color == sky.last_frame.ambient_directions[index], "interior directional ambient reaches host")
	check(sky.environment.background_mode == (Environment.BG_SKY if sky.last_frame.sky_visible else Environment.BG_COLOR), "interior background follows Show Sky independently of lighting")
	print("RND-18 RESULT: ", "FAIL" if failed else "PASS")
	scene.queue_free()
	await process_frame
	quit(1 if failed else 0)
