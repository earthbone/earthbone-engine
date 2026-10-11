extends SceneTree

## Sky-pass depth and hemisphere continuity checks, using Godot's documented EYEDIR.
const RecordSky := preload("res://record_sky.gd")
const PIXEL_TOLERANCE := 2.0 / 255.0
const EDGE_TOLERANCE := 0.04
const INSTALLED_EDGE_TOLERANCE := 0.08
const MIN_MOUNTAIN_PIXELS := 500
const MIN_SKY_PIXELS := 500
var failed := false
var scene := Node3D.new()
var sky = null
var camera := Camera3D.new()
var output := "res://../target/rnd_30"

func _initialize() -> void:
	call_deferred("_run")

func check(ok: bool, message: String) -> void:
	if not ok:
		failed = true
		printerr("RND-30 FAIL: ", message)

func difference(a: Color, b: Color) -> float:
	return maxf(absf(a.r - b.r), maxf(absf(a.g - b.g), absf(a.b - b.b)))

func capture(name: String) -> Image:
	for _frame in range(4):
		await process_frame
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	check(image.save_png(output + "/" + name + ".png") == OK, "save " + name)
	return image

func texture(color: Color) -> ImageTexture:
	var image := Image.create(64, 64, false, Image.FORMAT_RGBA8)
	image.fill(color)
	return ImageTexture.create_from_image(image)

func synthetic_sky() -> void:
	sky = RecordSky.new()
	scene.add_child(sky)
	sky.set_process(false)
	sky.fog_pass.hide()
	sky.environment.background_mode = Environment.BG_SKY
	for key in ["upper", "lower", "horizon"]:
		sky.material.set_shader_parameter(key, Color(0.1, 0.2, 0.3))
	var clouds := Image.create(64, 64, false, Image.FORMAT_RGBA8)
	for y in range(64):
		for x in range(64):
			var value := 0.6 + 0.2 * cos(TAU * x / 64.0) * cos(TAU * y / 64.0)
			clouds.set_pixel(x, y, Color(value, value, value, 1.0))
	clouds.generate_mipmaps()
	var array := Texture2DArray.new()
	check(array.create_from_images([clouds]) == OK, "synthetic cloud upload")
	sky.material.set_shader_parameter("cloud_textures", array)
	sky.material.set_shader_parameter("cloud_count", 1)
	var colors := PackedColorArray([Color.WHITE])
	colors.resize(256)
	var speeds := PackedVector2Array()
	speeds.resize(256)
	var layers := PackedFloat32Array()
	layers.resize(256)
	sky.material.set_shader_parameter("cloud_colors", colors)
	sky.material.set_shader_parameter("cloud_speeds", speeds)
	sky.material.set_shader_parameter("cloud_layers", layers)
	for body in ["sun", "masser", "secunda"]:
		sky.material.set_shader_parameter(body + "_texture", texture(Color.WHITE))
		sky.material.set_shader_parameter(body + "_enabled", true)
		var horizontal := {"sun": -0.25, "masser": 0.0, "secunda": 0.25}
		sky.material.set_shader_parameter(body + "_direction", Vector3(horizontal[body], 0.3, -1.0).normalized())
	sky.material.set_shader_parameter("sun_color", Color.WHITE)
	sky.material.set_shader_parameter("glare_texture", texture(Color.WHITE))
	sky.material.set_shader_parameter("sun_glare", 1.0)
	sky.material.set_shader_parameter("stars_texture", texture(Color(0.1, 0.1, 0.1)))
	sky.material.set_shader_parameter("stars_color", Color.WHITE)
	sky.material.set_shader_parameter("stars_enabled", true)
	sky.material.set_shader_parameter("night_alpha", 1.0)

func rnd_30_far_geometry_and_camera_translation() -> void:
	synthetic_sky()
	var colors: PackedColorArray = sky.material.get_shader_parameter("cloud_colors")
	colors[0].a = 0.3
	sky.material.set_shader_parameter("cloud_colors", colors)
	camera.rotation_degrees.x = 15.0
	var marker := ShaderMaterial.new()
	var shader := Shader.new()
	shader.code = "shader_type spatial; render_mode unshaded, fog_disabled; uniform vec3 marker_color : source_color; void fragment() { ALBEDO = marker_color; }"
	marker.shader = shader
	marker.set_shader_parameter("marker_color", Color(0.0, 0.8, 0.1))
	var mountain := MeshInstance3D.new()
	var mesh := QuadMesh.new()
	mountain.mesh = mesh
	mountain.material_override = marker
	scene.add_child(mountain)
	for distance in [10.0, 10000.0, 1000000.0]:
		# CONTEXT: Godot's culling projection needs a finite far/near ratio at extreme distances.
		camera.near = maxf(0.05, distance / 10000.0)
		camera.far = distance * 2.0
		mountain.position = camera.global_position - camera.global_basis.z * distance
		mountain.basis = camera.basis
		mesh.size = Vector2(distance * 0.5, distance * 0.5)
		var image := await capture("depth_" + str(int(distance)))
		var occluded := 0
		for y in range(160, 200):
			for x in range(300, 340):
				if difference(image.get_pixel(x, y), Color(0.0, 0.8, 0.1)) <= PIXEL_TOLERANCE:
					occluded += 1
		check(occluded == 1600, "all sky layers remain behind geometry at " + str(distance))
	mountain.hide()
	camera.near = 0.05
	camera.far = 2000.0
	var before := await capture("translation_origin")
	camera.position = Vector3(123456.0, 654321.0, -234567.0)
	var after := await capture("translation_far")
	var changed := 0
	for y in range(0, 360, 4):
		for x in range(0, 640, 4):
			if difference(before.get_pixel(x, y), after.get_pixel(x, y)) > PIXEL_TOLERANCE:
				changed += 1
	check(changed == 0, "sky, clouds, sun, moons and stars follow camera translation")
	print("RND-30 depth: three distances, translation changed=", changed)

func rnd_30_hemisphere_has_no_cube_edges_or_horizon_streaks() -> void:
	synthetic_sky()
	for body in ["sun", "masser", "secunda", "stars"]:
		sky.material.set_shader_parameter(body + "_enabled", false)
	for elevation in [0.0, 45.0, 89.0, -45.0]:
		for yaw in [45.0, 135.0, 225.0, 315.0]:
			camera.rotation_degrees = Vector3(elevation, yaw, 0.0)
			var image := await capture("edge_%s_%s" % [int(elevation), int(yaw)])
			var largest := 0.0
			for y in range(4, 356):
				largest = maxf(largest, difference(image.get_pixel(319, y), image.get_pixel(320, y)))
				largest = maxf(largest, difference(image.get_pixel(320, y), image.get_pixel(320, y + 1)))
			check(largest <= EDGE_TOLERANCE, "continuous former cube edge and horizon at %s/%s: %s" % [elevation, yaw, largest])
	print("RND-30 continuity: sixteen directions, tolerance=", EDGE_TOLERANCE)

func rnd_30_installed_whiterun_mountain_and_sky_texel_masks() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("RND-30 SKIP: SKYRIM_DATA unset")
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	scene.add_child(streamer)
	check(streamer.call("load_data", data), "installed data")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	scene.add_child(player)
	check(player.call("use_streamer_data", streamer), "shared source")
	check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "Bannered Mare")
	check(player.call("start_gameplay"), "gameplay")
	var door := {}
	for candidate: Dictionary in player.call("gameplay_doors"):
		if not candidate.locked and candidate.get("far_worldspace", "") == "WhiterunWorld":
			door = candidate
			break
	check(not door.is_empty(), "city door")
	if door.is_empty():
		return
	check(player.call("activate_reference", door.file, door.object), "Whiterun")
	player.call("advance", 0.0)
	for _frame in range(45):
		player.call("advance", 1.0 / 60.0)
	var gate: Dictionary = player.call("detection_gate_reference")
	check(not gate.is_empty(), "wall gate")
	if gate.is_empty():
		return
	check(player.call("queue_player_input", {"action": "move_to", "file": gate.file, "object": gate.object, "offset": Vector3(0, 0, 1200), "match_rotation": false}), "wall viewpoint")
	check(player.call("step_player_input"), "apply wall viewpoint")
	player.call("advance", 0.0)
	for _frame in range(45):
		player.call("advance", 1.0 / 60.0)
	var host_camera := root.get_camera_3d()
	camera.global_position = host_camera.global_position
	camera.far = 10000.0
	camera.look_at(camera.position + Vector3(0.7, 0.05, 0.7))
	camera.make_current()
	sky = RecordSky.new()
	scene.add_child(sky)
	sky.set_process(false)
	check(sky.use_player(player), "same record sky as walkthrough")
	check(sky.last_frame.weather_sky and sky.last_frame.sky_visible, "gameplay exterior sky")
	check(sky.use_streamer(streamer, "WhiterunOrigin"), "preview controls over the shared record source")
	check(sky.records.set_time(12.0, 0.0), "noon")
	check(sky.records.set_weather("SkyrimCloudy", true) and sky.refresh(), "vanilla cloudy WTHR")
	check(int(streamer.call("terrain_lod_stats").get("instances", 0)) > 0, "installed BTR mountains")
	await capture("whiterun_cloudy")
	# CONTEXT: Freeze object motion and shader TIME so the captures isolate sky changes.
	scene.process_mode = Node.PROCESS_MODE_DISABLED
	Engine.time_scale = 0.0
	# CONTEXT: A depth mask makes terrain and sky classifications independent of their colors.
	sky.fog_pass.hide()
	sky.environment.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	var baseline := await capture("whiterun_no_fog")
	streamer.call("set_terrain_lod_visible", false)
	var no_terrain := await capture("whiterun_no_terrain")
	streamer.call("set_terrain_lod_visible", true)
	var probe := MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = Vector2(2, 2)
	probe.mesh = quad
	probe.extra_cull_margin = 1000000.0
	probe.ignore_occlusion_culling = true
	var material := ShaderMaterial.new()
	var shader := Shader.new()
	shader.code = "shader_type spatial; render_mode unshaded, depth_draw_never, depth_test_disabled, cull_disabled, fog_disabled; uniform sampler2D depth_texture : hint_depth_texture, repeat_disable, filter_nearest; void vertex() { POSITION = vec4(VERTEX.xy, 1.0, 1.0); } void fragment() { ALBEDO = vec3(texture(depth_texture, SCREEN_UV).r > 0.0 ? 1.0 : 0.0); ALPHA = 1.0; }"
	material.shader = shader
	material.render_priority = 127
	probe.material_override = material
	scene.add_child(probe)
	var mask := await capture("whiterun_depth_mask")
	probe.hide()
	for key in ["upper", "lower", "horizon"]:
		sky.material.set_shader_parameter(key, Color.MAGENTA)
	for body in ["sun", "masser", "secunda", "stars"]:
		sky.material.set_shader_parameter(body + "_enabled", false)
	sky.material.set_shader_parameter("cloud_count", 0)
	var diagnostic := await capture("whiterun_sky_marker")
	var mountain_pixels := 0
	var contaminated := 0
	var sky_pixels := 0
	var terrain_in_sky := 0
	for y in range(20, 240):
		for x in range(40, 600):
			# CONTEXT: Exclude silhouette samples whose antialiasing mixes both regions.
			var solid := mask.get_pixel(x, y).r > 0.99
			var empty := mask.get_pixel(x, y).r < 0.01
			for offset in [Vector2i(-1, 0), Vector2i(1, 0), Vector2i(0, -1), Vector2i(0, 1)]:
				solid = solid and mask.get_pixel(x + offset.x, y + offset.y).r > 0.99
				empty = empty and mask.get_pixel(x + offset.x, y + offset.y).r < 0.01
			if solid and difference(baseline.get_pixel(x, y), no_terrain.get_pixel(x, y)) > EDGE_TOLERANCE:
				mountain_pixels += 1
				if difference(baseline.get_pixel(x, y), diagnostic.get_pixel(x, y)) > PIXEL_TOLERANCE:
					contaminated += 1
			elif empty:
				sky_pixels += 1
				if difference(diagnostic.get_pixel(x, y), Color.MAGENTA) > PIXEL_TOLERANCE:
					terrain_in_sky += 1
	check(mountain_pixels >= MIN_MOUNTAIN_PIXELS and contaminated == 0, "mountain region excludes sky/cloud texels: %s pixels, %s contaminated" % [mountain_pixels, contaminated])
	check(sky_pixels >= MIN_SKY_PIXELS and terrain_in_sky == 0, "sky region excludes terrain texels: %s pixels, %s contaminated" % [sky_pixels, terrain_in_sky])
	print("RND-30 Whiterun: terrain=", mountain_pixels, " sky=", sky_pixels, " sky_on_terrain=", contaminated, " terrain_in_sky=", terrain_in_sky)
	sky._reset_textures()
	check(sky.refresh(), "restore installed cloud textures")
	sky.fog_pass.hide()
	# CONTEXT: Elevation separates the cloud survey from the installed terrain silhouettes.
	camera.position.y += 10000.0
	for overlay in root.find_children("*", "CanvasLayer", true, false):
		overlay.hide()
	for elevation in [0.0, 35.264]:
		for yaw in [45.0, 135.0, 225.0, 315.0]:
			camera.rotation_degrees = Vector3(elevation, yaw, 0.0)
			var image := await capture("installed_edge_%s_%s" % [int(elevation), int(yaw)])
			var largest := 0.0
			for y in range(20, 175):
				largest = maxf(largest, difference(image.get_pixel(319, y), image.get_pixel(320, y)))
				largest = maxf(largest, difference(image.get_pixel(320, y), image.get_pixel(320, y + 1)))
			check(largest <= INSTALLED_EDGE_TOLERANCE, "installed cloud continuity at %s/%s: %s" % [elevation, yaw, largest])
	print("RND-30 installed continuity: eight directions, tolerance=", INSTALLED_EDGE_TOLERANCE)
	# CONTEXT: Player teardown releases borrowed server instances before their source leaves the tree.
	player.free()
	streamer.free()

func _run() -> void:
	DirAccess.make_dir_recursive_absolute(output)
	root.add_child(scene)
	scene.add_child(camera)
	camera.make_current()
	match OS.get_environment("SOE_RND30_CASE"):
		"depth":
			await rnd_30_far_geometry_and_camera_translation()
		"edges":
			await rnd_30_hemisphere_has_no_cube_edges_or_horizon_streaks()
		"installed":
			await rnd_30_installed_whiterun_mountain_and_sky_texel_masks()
		_:
			check(false, "unknown test case")
	scene.queue_free()
	await process_frame
	print("RND-30 RESULT: ", "FAIL" if failed else "PASS")
	quit(1 if failed else 0)
