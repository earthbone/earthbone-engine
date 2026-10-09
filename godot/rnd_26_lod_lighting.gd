extends SceneTree

## RND-26 bounds fixed before captures: synthetic channel error <= 0.025
## linear, seam luminance ratio <= 1.35 and difference <= 0.08.
const RecordSky := preload("res://record_sky.gd")
const MAX_SEAM_RATIO := 1.35
const MAX_SEAM_DIFFERENCE := 0.08
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _capture(viewport: Viewport) -> Image:
	for frame in range(5):
		await process_frame
		RenderingServer.force_draw(false)
	return viewport.get_texture().get_image()

func _run() -> void:
	create_timer(600.0).timeout.connect(func():
		printerr("FAIL: RND-26 capture timed out")
		quit(1)
	)
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("CellStreamerNode"):
		printerr("FAIL: installed data and host extension required")
		quit(2)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "installed data opens")
	await _synthetic(streamer)
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared source")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "interior entry")
	_check(player.call("start_gameplay"), "gameplay starts")
	var door: Dictionary = {}
	for candidate: Dictionary in player.call("gameplay_doors"):
		if not candidate.locked and candidate.get("far_worldspace", "") == "WhiterunWorld":
			door = candidate
			break
	_check(not door.is_empty(), "city door found")
	if not door.is_empty():
		_check(player.call("activate_reference", door.file, door.object), "enter city")
		player.call("advance", 0.0)
		for frame in range(45):
			player.call("advance", 1.0 / 60.0)
		var gate: Dictionary = player.call("detection_gate_reference")
		_check(not gate.is_empty(), "gate found")
		if not gate.is_empty():
			_check(player.call("queue_player_input", {"action": "move_to", "file": gate.file, "object": gate.object, "offset": Vector3(0, 0, 1200), "match_rotation": false}), "move to gate wall")
			_check(player.call("step_player_input"), "movement applied")
			player.call("advance", 0.0)
			for frame in range(45):
				player.call("advance", 1.0 / 60.0)
			player.call("set_facing", PI, -0.18)
			# CONTEXT: A complete retained ring joins the BTR edge at y=-4.
			# Gameplay's one-cell load ring leaves a geometric gap here.
			streamer.call("clear")
			_check(streamer.call("stream_exterior", "WhiterunOrigin", 2) > 0, "complete LAND ring")
			_check(streamer.call("stream_terrain_lod", "Tamriel", 4, -2, 2, 16, 0) > 0, "adjacent BTR ring")
			var sky := RecordSky.new()
			root.add_child(sky)
			sky.set_process(false)
			_check(sky.use_streamer(streamer, "WhiterunOrigin", 12.0, 0.0), "record weather lighting")
			var stats: Dictionary = streamer.call("terrain_lod_stats")
			print("RND-26 camera=", player.call("position"), " lod_instances=", stats.get("instances"), " near_cells=", player.call("exterior_resources").size())
			streamer.call("set_terrain_lod_visible", false)
			var control := await _capture(root)
			streamer.call("set_terrain_lod_visible", true)
			var image := await _capture(root)
			streamer.call("set_loaded_terrain_visible", false)
			var no_near := await _capture(root)
			streamer.call("set_loaded_terrain_visible", true)
			_check(image.save_png("res://.godot/rnd_26-whiterun-seam.png") == OK, "seam capture saves")
			_check(control.save_png("res://.godot/rnd_26-whiterun-no-lod.png") == OK, "LOD mask control saves")
			_check(no_near.save_png("res://.godot/rnd_26-whiterun-no-land.png") == OK, "LAND mask control saves")
			_seam_brightness(image, control, no_near, streamer.call("loaded_terrain_meshes"))
			sky.free()
	player.free()
	streamer.free()
	print("rnd_26_lod_lighting: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)

func _synthetic(streamer: Node3D) -> void:
	var meshes: Array = streamer.call("model_meshes", "terrain/tamriel/tamriel.4.0.-8.btr")
	var material: ShaderMaterial
	for mesh: ArrayMesh in meshes:
		if mesh.surface_get_material(0) is ShaderMaterial:
			material = mesh.surface_get_material(0).duplicate() as ShaderMaterial
			break
	_check(material != null, "shipped BTR selects model-space material")
	if material == null:
		return
	_check(material.get_shader_parameter("has_noise") == true, "BTR terrain noise bound")
	_check(material.get_shader_parameter("noise_texture") is Texture2D, "BTR noise uploads")
	material.set_shader_parameter("has_noise", false)
	_check(material.get_shader_parameter("has_normal") == true, "BTR normal texture bound")
	_check(material.get_shader_parameter("normal_texture") is Texture2D, "BTR normal uploads")
	_check(material.get_shader_parameter("diffuse_texture") is Texture2D, "BTR diffuse uploads")
	var viewport := SubViewport.new()
	viewport.size = Vector2i(128, 128)
	viewport.own_world_3d = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 2.0
	camera.position = Vector3(0, 2, 0)
	camera.look_at_from_position(camera.position, Vector3.ZERO, Vector3.FORWARD)
	viewport.add_child(camera)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color.WHITE
	environment.environment.ambient_light_energy = 0.0
	environment.environment.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	viewport.add_child(environment)
	var light := DirectionalLight3D.new()
	light.rotation_degrees.x = -90
	light.light_energy = 1.0
	viewport.add_child(light)
	var plane := MeshInstance3D.new()
	plane.mesh = PlaneMesh.new()
	plane.material_override = material
	viewport.add_child(plane)
	var reference := MeshInstance3D.new()
	reference.mesh = PlaneMesh.new()
	var standard := StandardMaterial3D.new()
	standard.albedo_color = Color(0.25, 0.25, 0.25)
	standard.roughness = 1.0
	standard.metallic_specular = 0.0
	reference.material_override = standard
	viewport.add_child(reference)
	var diffuse := Image.create(1, 1, false, Image.FORMAT_RGBA8)
	diffuse.fill(Color(0.25, 0.25, 0.25))
	material.set_shader_parameter("diffuse_texture", ImageTexture.create_from_image(diffuse))
	var normal := Image.create(1, 1, false, Image.FORMAT_RGBA8)
	for direction in [Vector3.UP, Vector3.RIGHT, Vector3.FORWARD, Vector3.BACK, Vector3.DOWN, Vector3(0.6, 0.8, 0)]:
		# CONTEXT: Texture RGB is Skyrim X,Z,Y, Godot X,Y,-Z.
		normal.fill(Color(direction.x * 0.5 + 0.5, direction.y * 0.5 + 0.5, -direction.z * 0.5 + 0.5))
		material.set_shader_parameter("normal_texture", ImageTexture.create_from_image(normal))
		standard.albedo_color = Color(0.25, 0.25, 0.25)
		if direction == Vector3.UP:
			light.rotation_degrees = Vector3(-90, 0, 0)
		else:
			light.basis = Basis.looking_at(-direction, Vector3.FORWARD if absf(direction.y) > 0.99 else Vector3.UP)
		plane.visible = true
		reference.visible = false
		var actual := await _capture(viewport)
		# CONTEXT: Rotate only the reference normal, leaving its visible geometry flat.
		var arrays := (reference.mesh as PlaneMesh).get_mesh_arrays()
		var normals: PackedVector3Array = arrays[Mesh.ARRAY_NORMAL]
		for index in normals.size():
			normals[index] = direction
		arrays[Mesh.ARRAY_NORMAL] = normals
		var mesh := ArrayMesh.new()
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
		reference.mesh = mesh
		plane.visible = false
		reference.visible = true
		var expected := await _capture(viewport)
		var a := actual.get_pixel(64, 64).srgb_to_linear()
		var b := expected.get_pixel(64, 64).srgb_to_linear()
		_check(absf(a.r - b.r) <= 0.025, "normal %s actual=%f expected=%f" % [direction, a.r, b.r])
		_check(a.r > 0.005, "normal %s receives no aligned sun" % direction)
		print("rnd_26_model_normal ", direction, " linear=", a.r, " reference=", b.r)
		reference.mesh = PlaneMesh.new()
	normal.fill(Color(0.5, 1.0, 0.5))
	material.set_shader_parameter("normal_texture", ImageTexture.create_from_image(normal))
	for state in ["ambient", "fog", "rotated", "missing"]:
		light.rotation_degrees = Vector3(-90, 0, 0)
		light.visible = state != "ambient"
		environment.environment.ambient_light_energy = 0.6 if state == "ambient" else 0.2
		environment.environment.fog_enabled = state == "fog"
		environment.environment.fog_density = 0.25
		environment.environment.fog_light_color = Color(0.1, 0.2, 0.3)
		material.set_shader_parameter("has_normal", state != "missing")
		if state == "rotated":
			plane.rotation_degrees = Vector3(15, 35, 0)
			reference.rotation_degrees = plane.rotation_degrees
			camera.position = Vector3(0, 2, 1)
			camera.look_at(Vector3.ZERO)
		plane.visible = true
		reference.visible = false
		var actual := await _capture(viewport)
		plane.visible = false
		reference.visible = true
		var expected := await _capture(viewport)
		var a := actual.get_pixel(64, 64).srgb_to_linear()
		var b := expected.get_pixel(64, 64).srgb_to_linear()
		_check(absf(a.r - b.r) <= 0.025 and absf(a.g - b.g) <= 0.025 and absf(a.b - b.b) <= 0.025, "%s lighting actual=%s expected=%s" % [state, a, b])
		print("rnd_26_shared_", state, " linear=", a, " reference=", b)
	# CONTEXT: Reference HLSL applies noise in linear color, before lighting.
	light.rotation_degrees = Vector3(-90, 0, 0)
	light.visible = true
	environment.environment.ambient_light_energy = 0.0
	environment.environment.fog_enabled = false
	plane.rotation = Vector3.ZERO
	reference.rotation = Vector3.ZERO
	camera.look_at_from_position(Vector3(0, 2, 0), Vector3.ZERO, Vector3.FORWARD)
	material.set_shader_parameter("has_normal", true)
	material.set_shader_parameter("has_noise", true)
	for tint in [Color(0.25, 0.25, 0.25), Color(0.9, 0.8, 0.7)]:
		diffuse.fill(tint)
		material.set_shader_parameter("diffuse_texture", ImageTexture.create_from_image(diffuse))
		var linear: Color = diffuse.get_pixel(0, 0).srgb_to_linear()
		var parameter := clampf(1.6666666 * (0.55 * (linear.r + linear.g + linear.b) - 0.4), 0.0, 1.0)
		parameter = parameter * parameter * (3.0 - 2.0 * parameter) * 0.8
		for value in [0.0, 0.5, 1.0]:
			var mask := Image.create(1, 1, false, Image.FORMAT_RGBA8)
			mask.fill(Color(value, value, value))
			material.set_shader_parameter("noise_texture", ImageTexture.create_from_image(mask))
			var stored := mask.get_pixel(0, 0).r
			var factor := 0.8333333 * (parameter * (0.37 - stored) + stored) + 0.37
			standard.albedo_color = Color(linear.r * factor, linear.g * factor, linear.b * factor).linear_to_srgb()
			plane.visible = true
			reference.visible = false
			var actual := await _capture(viewport)
			plane.visible = false
			reference.visible = true
			var expected := await _capture(viewport)
			var a := actual.get_pixel(64, 64).srgb_to_linear()
			var b := expected.get_pixel(64, 64).srgb_to_linear()
			_check(absf(a.r - b.r) <= 0.025 and absf(a.g - b.g) <= 0.025 and absf(a.b - b.b) <= 0.025, "linear noise tint=%s mask=%f actual=%s expected=%s" % [tint, value, a, b])
			print("rnd_26_noise tint=", tint, " mask=", value, " linear=", a, " reference=", b)
	viewport.free()

func _luminance(color: Color) -> float:
	var linear := color.srgb_to_linear()
	return 0.2126 * linear.r + 0.7152 * linear.g + 0.0722 * linear.b

func _lod_pixel(image: Image, control: Image, x: int, y: int) -> bool:
	var a := image.get_pixel(x, y)
	var b := control.get_pixel(x, y)
	return absf(a.r - b.r) + absf(a.g - b.g) + absf(a.b - b.b) > 0.06

func _seam_brightness(image: Image, control: Image, no_near: Image, meshes: Array) -> void:
	var near_sum := 0.0
	var lod_sum := 0.0
	var samples := 0
	var edge := {}
	var camera := root.get_camera_3d()
	# CONTEXT: Project the actual shared y=-4 cell boundary. A hill crest
	# can hide distant LOD but is not the LAND/BTR seam.
	for mesh: ArrayMesh in meshes:
		var arrays := mesh.surface_get_arrays(0)
		var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
		for vertex in vertices:
			if absf(vertex.z - 4.0 * 4096.0 * 0.0142875) > 0.001:
				continue
			if camera.is_position_behind(vertex):
				continue
			var pixel := camera.unproject_position(vertex)
			edge[roundi(pixel.x)] = roundi(pixel.y)
	print("rnd_26_projected_edge points=", edge.size())
	for x: int in edge:
		var center: int = edge[x]
		if x < 40 or x >= 600 or center < 6 or center >= 354:
			continue
		for y in range(center - 2, center + 3):
			if not _lod_pixel(image, control, x, y - 3) or _lod_pixel(image, control, x, y + 3):
				continue
			if not _lod_pixel(image, no_near, x, y + 3):
				continue
			var far_value := _luminance(image.get_pixel(x, y - 3))
			var near_value := _luminance(image.get_pixel(x, y + 3))
			if far_value < 0.01 or near_value < 0.01:
				continue
			near_sum += near_value
			lod_sum += far_value
			samples += 1
			break
	_check(samples >= 40, "seam has too few measured columns: %d" % samples)
	if samples == 0:
		return
	var near_mean := near_sum / samples
	var lod_mean := lod_sum / samples
	var ratio := maxf(near_mean, lod_mean) / minf(near_mean, lod_mean)
	var difference := absf(near_mean - lod_mean)
	print("rnd_26_seam samples=", samples, " near_linear=", near_mean, " lod_linear=", lod_mean, " ratio=", ratio, " difference=", difference)
	_check(ratio <= MAX_SEAM_RATIO and difference <= MAX_SEAM_DIFFERENCE, "seam brightness exceeds ratio 1.35 or linear difference 0.08")
