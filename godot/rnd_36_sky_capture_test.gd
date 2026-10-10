extends SceneTree

## Directional sky captures using installed WTHR/CLMT or a smooth diagnostic texture.
const RecordSky := preload("res://record_sky.gd")
const PIXEL_TOLERANCE := 2.0 / 255.0
const CENTRE_TOLERANCE := 0.04
const MIN_NIGHT_PIXELS := 200
var failed := false
var scene := Node3D.new()
var camera := Camera3D.new()
var sky = null
var output := "res://../target/rnd_36"

func _initialize() -> void:
	call_deferred("_run")

func check(ok: bool, message: String) -> void:
	if not ok:
		failed = true
		printerr("RND-36 FAIL: ", message)

func difference(a: Color, b: Color) -> float:
	return maxf(absf(a.r - b.r), maxf(absf(a.g - b.g), absf(a.b - b.b)))

func capture(name: String) -> Image:
	for _frame in range(8):
		await process_frame
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	check(image.save_png(output + "/" + name + ".png") == OK, "save " + name)
	return image

func centre_jump(image: Image, baseline: Image = null) -> float:
	var largest := 0.0
	for y in range(176, 184):
		for x in range(316, 324):
			var centre := image.get_pixel(x, y)
			var right := image.get_pixel(x + 1, y)
			var below := image.get_pixel(x, y + 1)
			if baseline != null:
				centre -= baseline.get_pixel(x, y)
				right -= baseline.get_pixel(x + 1, y)
				below -= baseline.get_pixel(x, y + 1)
			largest = maxf(largest, difference(centre, right))
			largest = maxf(largest, difference(centre, below))
	return largest

func changed_pixels(a: Image, b: Image) -> int:
	var changed := 0
	for y in range(20, 340):
		for x in range(20, 620):
			if difference(a.get_pixel(x, y), b.get_pixel(x, y)) > PIXEL_TOLERANCE:
				changed += 1
	return changed

func diagnostic_texture() -> ImageTexture:
	var image := Image.create(256, 256, false, Image.FORMAT_RGBA8)
	for y in range(256):
		for x in range(256):
			image.set_pixel(x, y, Color(0.2 + 0.4 * x / 255.0, 0.2 + 0.4 * y / 255.0, 0.2, 1.0))
	image.generate_mipmaps()
	return ImageTexture.create_from_image(image)

func rnd_36_installed_dome_uv_continuity() -> void:
	var image := Image.create(256, 256, false, Image.FORMAT_RGBA8)
	for y in range(256):
		for x in range(256):
			image.set_pixel(x, y, Color(0.4 + 0.1 * cos(TAU * x / 256.0), 0.4 + 0.1 * cos(TAU * y / 256.0), 0.4, 1.0))
	image.generate_mipmaps()
	var texture := ImageTexture.create_from_image(image)
	for body in ["sun", "masser", "secunda"]:
		sky.material.set_shader_parameter(body + "_enabled", false)
	for layer in sky.night_layers:
		layer.hide()
		for body in ["sun", "masser", "secunda"]:
			layer.material_override.set_shader_parameter(body + "_enabled", false)
	for layer in sky.night_layers:
		if layer.name not in ["SkyrimGalaxy", "SkyrimStars_0"]:
			continue
		layer.show()
		layer.material_override.set_shader_parameter("night_texture", texture)
		layer.material_override.set_shader_parameter("night_alpha", 1.0)
		var diagnostic := await capture("whiterun_midnight_uv_" + layer.name)
		var jump := centre_jump(diagnostic)
		check(jump <= CENTRE_TOLERANCE, "loaded dome UVs close at zenith: %s step=%s" % [layer.name, jump])
		print("RND-36 installed UV ", layer.name, " centre_jump=", jump)
		layer.hide()

func rnd_36_synthetic_night_layers_close_at_zenith_and_stay_behind_geometry() -> void:
	sky = RecordSky.new()
	scene.add_child(sky)
	sky.set_process(false)
	sky.fog_pass.hide()
	sky.environment.background_mode = Environment.BG_SKY
	for key in ["upper", "lower", "horizon"]:
		sky.material.set_shader_parameter(key, Color(0.1, 0.1, 0.1))
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = PackedVector3Array([Vector3(-4, 1, -4), Vector3(4, 1, -4), Vector3(4, 1, 4), Vector3(-4, 1, 4)])
	arrays[Mesh.ARRAY_TEX_UV] = PackedVector2Array([Vector2.ZERO, Vector2.RIGHT, Vector2.ONE, Vector2.DOWN])
	arrays[Mesh.ARRAY_INDEX] = PackedInt32Array([0, 1, 2, 0, 2, 3])
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	var night := MeshInstance3D.new()
	night.mesh = mesh
	night.extra_cull_margin = 1000000.0
	var material := ShaderMaterial.new()
	material.shader = RecordSky.NIGHT_SHADER
	material.set_shader_parameter("night_texture", diagnostic_texture())
	material.set_shader_parameter("stars_color", Color.WHITE)
	material.set_shader_parameter("night_alpha", 0.0)
	night.material_override = material
	scene.add_child(night)
	var noon := await capture("synthetic_noon")
	night.hide()
	var baseline := await capture("synthetic_noon_without_night")
	check(changed_pixels(noon, baseline) == 0, "synthetic day excludes night texels")
	night.show()
	material.set_shader_parameter("night_alpha", 1.0)
	for yaw in [0.0, 90.0, 180.0, 270.0]:
		camera.rotation_degrees = Vector3(90.0, yaw, 0.0)
		var image := await capture("synthetic_midnight_%s" % int(yaw))
		var jump := centre_jump(image)
		check(jump <= CENTRE_TOLERANCE, "smooth night UVs at zenith yaw %s: %s" % [yaw, jump])
		check(changed_pixels(image, noon) >= MIN_NIGHT_PIXELS, "synthetic midnight contains night texels")
		print("RND-36 synthetic zenith yaw=", yaw, " centre_jump=", jump)
	var before := await capture("synthetic_before_translation")
	camera.position = Vector3(12345, 67890, -45678)
	var after := await capture("synthetic_after_translation")
	check(changed_pixels(before, after) == 0, "night sky follows camera translation")
	var marker := MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = Vector2(5, 5)
	marker.mesh = quad
	var marker_material := StandardMaterial3D.new()
	marker_material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	marker_material.albedo_color = Color(0.0, 0.8, 0.1)
	marker.material_override = marker_material
	scene.add_child(marker)
	marker.position = camera.position - camera.basis.z * 10.0
	marker.basis = camera.basis
	var occluded := await capture("synthetic_world_occlusion")
	check(difference(occluded.get_pixel(320, 180), marker_material.albedo_color) <= PIXEL_TOLERANCE, "world geometry occludes night layer")
	marker.hide()
	var opaque := Image.create(64, 64, false, Image.FORMAT_RGBA8)
	opaque.fill(Color.WHITE)
	material.set_shader_parameter("masser_enabled", true)
	material.set_shader_parameter("masser_direction", Vector3.UP)
	material.set_shader_parameter("masser_texture", ImageTexture.create_from_image(opaque))
	var moon := await capture("synthetic_moon_occlusion")
	check(difference(moon.get_pixel(320, 180), noon.get_pixel(320, 180)) <= PIXEL_TOLERANCE, "opaque moon disc masks night texels")
	material.set_shader_parameter("masser_enabled", false)
	var clouds := Texture2DArray.new()
	check(clouds.create_from_images([opaque]) == OK, "opaque cloud texture")
	var colors := PackedColorArray([Color.WHITE])
	colors.resize(256)
	material.set_shader_parameter("cloud_textures", clouds)
	material.set_shader_parameter("cloud_count", 1)
	material.set_shader_parameter("cloud_colors", colors)
	var cloudy := await capture("synthetic_cloud_occlusion")
	check(changed_pixels(cloudy, noon) == 0, "opaque clouds mask night texels")

func rnd_36_installed_whiterun_noon_midnight_night_layers_and_zenith() -> void:
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	scene.add_child(streamer)
	if not streamer.call("load_data", OS.get_environment("SKYRIM_DATA")):
		check(false, "load installed data")
		return
	check(streamer.call("stream_exterior", "WhiterunOrigin", 0) > 0, "Whiterun exterior")
	var bounds: AABB = streamer.call("scene_bounds")
	camera.position = bounds.get_center() + Vector3(0.0, bounds.size.y * 0.5 + 8.0, 0.0)
	sky = RecordSky.new()
	scene.add_child(sky)
	sky.set_process(false)
	check(sky.use_streamer(streamer, "WhiterunOrigin"), "Whiterun record sky")
	check(sky.night_layers.size() == 4, "all four Stars.nif meshes")
	var names: Array[String] = []
	for layer in sky.night_layers:
		names.append(layer.name)
	check("SkyrimGalaxy" in names and "SkyrimStars_0" in names, "galaxy and stars NIF names: " + str(names))
	check(sky.records.set_weather("SkyrimClear", true), "select vanilla clear weather")
	for hour in [12.0, 0.0]:
		check(sky.records.set_time(hour, 0.0) and sky.refresh(), "sample climate")
		check(is_equal_approx(sky.last_frame.night_alpha, 0.0 if hour == 12.0 else 1.0), "climate noon/midnight fade")
		var name := "whiterun_%s" % ("noon" if hour == 12.0 else "midnight")
		var image := await capture(name)
		if hour == 12.0:
			check(centre_jump(image) <= CENTRE_TOLERANCE, "no radial seam in " + name)
		for layer in sky.night_layers:
			check(layer.visible == (hour == 0.0), "climate fade hides night mesh in daylight")
			check(is_equal_approx(layer.material_override.get_shader_parameter("night_alpha"), sky.last_frame.night_alpha), "night mesh uses shared World fade")
			layer.material_override.set_shader_parameter("night_alpha", 0.0)
		var baseline := await capture(name + "_without_night")
		var changed := changed_pixels(image, baseline)
		check(changed == 0 if hour == 12.0 else changed >= MIN_NIGHT_PIXELS, "night texel mask " + name + ": " + str(changed))
		var jump := centre_jump(image, baseline)
		check(jump <= CENTRE_TOLERANCE, "night contribution has no radial seam in " + name)
		print("RND-36 installed ", name, " night_pixels=", changed, " night_centre_jump=", jump, " baseline_centre_jump=", centre_jump(baseline), " layers=", sky.night_layers.size())
	await rnd_36_installed_dome_uv_continuity()
	streamer.free()

func _run() -> void:
	DirAccess.make_dir_recursive_absolute(output)
	root.add_child(scene)
	scene.add_child(camera)
	camera.rotation_degrees.x = 90.0
	camera.make_current()
	match OS.get_environment("SOE_RND36_CASE"):
		"synthetic":
			await rnd_36_synthetic_night_layers_close_at_zenith_and_stay_behind_geometry()
		"installed":
			await rnd_36_installed_whiterun_noon_midnight_night_layers_and_zenith()
		_:
			check(false, "unknown case")
	scene.queue_free()
	await process_frame
	print("RND-36 RESULT: ", "FAIL" if failed else "PASS")
	quit(1 if failed else 0)
