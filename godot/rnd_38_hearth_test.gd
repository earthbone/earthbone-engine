extends "res://walkthrough.gd"

## Captures all resident hearth effect layers after the playable walkthrough starts.
const MIN_FIRE_PIXELS := 100
const MIN_EDGE_PIXELS := 20
const COLOR_TOLERANCE := 0.20
const ADDITIVE_TOLERANCE := 0.035
var failures := 0
var hearth: Array[Dictionary] = []
var palette_colors: Array[Vector3] = []
var expected_palettes := {}
var backdrop: MeshInstance3D
var backdrop_material: ShaderMaterial
var output := "res://../target/rnd_38"

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("RND-38 FAIL: ", message)

func capture(name: String) -> Image:
	for _frame in range(4):
		await process_frame
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	check(image.save_png(output + "/" + name + ".png") == OK, "save " + name)
	return image

func visible_hearth(visible: bool) -> void:
	for row in hearth:
		RenderingServer.instance_set_visible(row.rid, visible)

func rgb(pixel: Color) -> Vector3:
	var linear := pixel.srgb_to_linear()
	return Vector3(linear.r, linear.g, linear.b)

func strength(color: Vector3) -> float:
	return maxf(color.x, maxf(color.y, color.z))

func palette_distance(color: Vector3) -> float:
	color = color.max(Vector3.ZERO)
	var sum := color.x + color.y + color.z
	if sum < 0.0001:
		return 0.0
	color /= sum
	var distance := INF
	for palette in palette_colors:
		distance = minf(distance, color.distance_to(palette))
	return distance

func read_palette(texture: Texture2D, path: String) -> void:
	var image := texture.get_image()
	if image.is_compressed():
		check(image.decompress() == OK, "palette decompresses")
	if not expected_palettes.is_empty():
		var key := path.to_lower().replace("\\", "/")
		check(expected_palettes.has(key), "bound palette has an authored DDS: " + path)
		if expected_palettes.has(key):
			var expected: Dictionary = expected_palettes[key]
			check(image.get_width() == int(expected.width) and image.get_height() == int(expected.height), "palette dimensions match authored DDS: " + path)
			for sample: Array in expected.samples:
				var actual := image.get_pixel(int(sample[0]), int(sample[1]))
				var wanted := Color(float(sample[2]) / 255.0, float(sample[3]) / 255.0, float(sample[4]) / 255.0, float(sample[5]) / 255.0)
				check(Vector4(actual.r - wanted.r, actual.g - wanted.g, actual.b - wanted.b, actual.a - wanted.a).abs().length() <= 0.016, "bound palette pixels match authored DDS: " + path)
	var distinct := {}
	for y in range(image.get_height()):
		for x in range(image.get_width()):
			var color := rgb(image.get_pixel(x, y))
			var sum := color.x + color.y + color.z
			if sum < 0.001:
				continue
			color /= sum
			var key := Vector3i(color * 64.0)
			if not distinct.has(key):
				distinct[key] = true
				palette_colors.append(color)

func _start() -> void:
	await super._start()
	if not loaded:
		return
	# The visual check owns shutdown and does not need the profiling exit dump.
	player.tree_exiting.disconnect(Callable(system_overlay, "_write_on_player_exit"))
	player.set_process(false)
	player.set_physics_process(false)
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(output))
	for environment_node: WorldEnvironment in root.find_children("*", "WorldEnvironment", true, false):
		environment_node.environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
		environment_node.environment.glow_enabled = false
		environment_node.environment.fog_enabled = false
	for node: Node in root.find_children("*", "Node3D", true, false):
		if node.get_script() == RecordSky:
			node.set_process(false)
			node.get("fog_pass").hide()
	var expected_path := OS.get_environment("SOE_RND_38_PALETTES")
	if not expected_path.is_empty():
		var expected = JSON.parse_string(FileAccess.get_file_as_string(expected_path))
		check(expected is Dictionary and not expected.is_empty(), "reference DDS palette samples load")
		if expected is Dictionary:
			expected_palettes = expected
	var legacy_inspection := not streamer.has_method("nif_surfaces")
	var surfaces: Array = streamer.call("effect_surfaces" if legacy_inspection else "nif_surfaces")
	if legacy_inspection:
		for mesh: ArrayMesh in streamer.call("model_meshes", "Clutter/WoodFires/FireplaceWood01Burning.nif"):
			if mesh.get_aabb().size.y > 2.0:
				check(mesh.surface_get_material(0) is ShaderMaterial, "normal-map hull bypasses effect/distortion shading through StandardMaterial3D")
	var bounds := AABB()
	var effect_bounds_set := false
	var palettes := {}
	for row: Dictionary in surfaces:
		if legacy_inspection:
			row.effect = true
			row.refraction = false
		if not row.models.has("clutter/woodfires/fireplacewood01burning.nif"):
			continue
		if not row.effect and not str(row.source).to_lower().contains("vaportilenormal"):
			continue
		hearth.append(row)
		if row.effect:
			bounds = bounds.merge(row.bounds) if effect_bounds_set else row.bounds
			effect_bounds_set = true
		var material := row.material as ShaderMaterial
		check(material != null, "all visible hearth layers use their NIF shader path: " + row.source)
		if material == null:
			continue
		print("RND-38 layer source=", row.source, " palette=", row.palette,
			" flags=", row.flags1, "/", row.flags2, " blend=", row.src_blend, "/", row.dst_blend,
			" bounds=", row.bounds, " palette_bound=", material.get_shader_parameter("palette_texture") != null)
		check(material.shader.code.contains("depth_draw_never"), "hearth layer does not write depth: " + row.source)
		if row.effect:
			check(material.shader.code.contains("angle_opacity = mix("), "hearth retains falloff evaluation")
		else:
			check(row.refraction and material.shader.code.contains("ALBEDO = textureLod(screen_texture,"), "normal-map plane only distorts the background")
			check(material.render_priority < 0, "distortion precedes transparent flames")
		if not str(row.palette).is_empty():
			var palette := material.get_shader_parameter("palette_texture") as Texture2D
			check(palette != null, "authored palette is bound: " + row.palette)
			if palette != null and not palettes.has(row.palette):
				palettes[row.palette] = true
				read_palette(palette, str(row.palette))
	check(not hearth.is_empty(), "resident fireplace effect layers exist")
	check(hearth.any(func(row: Dictionary) -> bool: return (int(row.flags1) & 64) != 0), "authored falloff remains enabled")
	check(not palette_colors.is_empty(), "installed effects palette colours exist")
	if hearth.is_empty() or palette_colors.is_empty():
		quit(1)
		return
	var camera := root.get_camera_3d()
	if camera.environment != null:
		camera.environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
		camera.environment.tonemap_exposure = 1.0
		camera.environment.adjustment_enabled = false
		camera.environment.glow_enabled = false
		camera.environment.fog_enabled = false
	if camera.attributes != null:
		camera.attributes.auto_exposure_enabled = false
		camera.attributes.exposure_multiplier = 1.0
		camera.attributes.dof_blur_far_enabled = false
		camera.attributes.dof_blur_near_enabled = false
		camera.attributes.dof_blur_amount = 0.0
	var center := bounds.get_center()
	var extent := maxf(bounds.size.length(), 0.5)
	camera.global_position = center + Vector3(1, 0.3, 1).normalized() * extent * 1.6
	camera.look_at(center)
	camera.near = 0.02
	await capture("walkthrough_room")
	camera.global_position = center + Vector3(0.0, 0.8, -1.2) * extent * 0.65
	camera.look_at(center)
	await capture("walkthrough_hearth")
	if legacy_inspection:
		print("RESULT: ", "FAIL" if failures > 0 else "PASS", " rnd_33_bannered_mare_hearth_capture")
		quit(1 if failures > 0 else 0)
		return
	for canvas: CanvasLayer in root.find_children("*", "CanvasLayer", true, false):
		canvas.hide()
	# Keep the production instances and materials, removing room occlusion for measurement.
	for row: Dictionary in streamer.call("nif_surfaces"):
		RenderingServer.instance_set_visible(row.rid, hearth.any(func(layer: Dictionary) -> bool: return layer.rid == row.rid))
	backdrop = MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = Vector2.ONE * extent * 5.0
	backdrop.mesh = quad
	backdrop_material = ShaderMaterial.new()
	var shader := Shader.new()
	shader.code = "shader_type spatial; render_mode unshaded, cull_disabled, fog_disabled; uniform vec3 backdrop_color = vec3(0.0); void fragment() { ALBEDO = backdrop_color; }"
	backdrop_material.shader = shader
	backdrop.material_override = backdrop_material
	root.add_child(backdrop)
	var best_pixels := 0
	var best_direction := Vector3.FORWARD
	for direction in [Vector3.FORWARD, Vector3.BACK, Vector3.RIGHT, Vector3.LEFT]:
		camera.global_position = center + direction * extent * 1.6
		camera.look_at(center)
		backdrop.global_position = center - direction * extent * 0.6
		backdrop.look_at(camera.global_position)
		visible_hearth(false)
		var empty := await capture("view_%s_empty" % direction)
		visible_hearth(true)
		var fire := await capture("view_%s_fire" % direction)
		var count := 0
		for y in range(fire.get_height()):
			for x in range(fire.get_width()):
				if strength(rgb(fire.get_pixel(x, y)) - rgb(empty.get_pixel(x, y))) > 0.025:
					count += 1
		if count > best_pixels:
			best_pixels = count
			best_direction = direction
	check(best_pixels >= MIN_FIRE_PIXELS, "visible fire contribution: %d" % best_pixels)
	camera.global_position = center + best_direction * extent * 1.6
	camera.look_at(center)
	backdrop.global_position = center - best_direction * extent * 0.6
	backdrop.look_at(camera.global_position)
	visible_hearth(false)
	var dark_empty := await capture("dark_empty")
	visible_hearth(true)
	var dark_fire := await capture("dark_fire")
	backdrop_material.set_shader_parameter("backdrop_color", Vector3(0.08, 0.12, 0.16))
	visible_hearth(false)
	var light_empty := await capture("light_empty")
	visible_hearth(true)
	var light_fire := await capture("light_fire")
	var all_hue_errors := 0
	var all_fire_pixels := 0
	for y in range(dark_fire.get_height()):
		for x in range(dark_fire.get_width()):
			var color := rgb(dark_fire.get_pixel(x, y)) - rgb(dark_empty.get_pixel(x, y))
			if strength(color) > 0.025:
				all_fire_pixels += 1
				if palette_distance(color) > COLOR_TOLERANCE:
					all_hue_errors += 1
	check(all_fire_pixels >= MIN_FIRE_PIXELS, "all hearth layers produce visible flames")
	check(all_hue_errors == 0, "whole hearth has only palette hues, including the normal-map plane: %d of %d" % [all_hue_errors, all_fire_pixels])
	print("RND-38 whole_hearth_pixels=", all_fire_pixels, " whole_hearth_palette_errors=", all_hue_errors)
	for row: Dictionary in streamer.call("nif_surfaces"):
		RenderingServer.instance_set_visible(row.rid, hearth.any(func(layer: Dictionary) -> bool: return layer.rid == row.rid and layer.refraction))
	var haze_light := await capture("refraction_light")
	var opaque_haze_pixels := 0
	for y in range(haze_light.get_height()):
		for x in range(haze_light.get_width()):
			var contrast := rgb(light_empty.get_pixel(x, y)) - rgb(dark_empty.get_pixel(x, y))
			if strength(contrast) >= 0.06 and strength((rgb(haze_light.get_pixel(x, y)) - rgb(light_empty.get_pixel(x, y))).abs()) > ADDITIVE_TOLERANCE:
				opaque_haze_pixels += 1
	check(opaque_haze_pixels == 0, "refraction transmits a flat background without an opaque silhouette: %d" % opaque_haze_pixels)
	print("RND-38 opaque_haze_pixels=", opaque_haze_pixels)
	var falloff_materials := {}
	for row: Dictionary in streamer.call("nif_surfaces"):
		var selected := hearth.any(func(layer: Dictionary) -> bool: return layer.rid == row.rid and (int(layer.flags1) & 64) != 0)
		RenderingServer.instance_set_visible(row.rid, selected)
		if selected:
			var material := row.material as ShaderMaterial
			var id := material.get_instance_id()
			if not falloff_materials.has(id):
				falloff_materials[id] = [material, material.get_shader_parameter("falloff")]
				material.set_shader_parameter("falloff", Vector4(0.0, 1.0, 0.0, 0.0))
	backdrop_material.set_shader_parameter("backdrop_color", Vector3.ZERO)
	var falloff_zero := await capture("falloff_zero")
	for values: Array in falloff_materials.values():
		values[0].set_shader_parameter("falloff", Vector4(0.0, 1.0, 1.0, 1.0))
	var falloff_full := await capture("falloff_full")
	var falloff_pixels := 0
	for y in range(falloff_full.get_height()):
		for x in range(falloff_full.get_width()):
			if strength(rgb(falloff_full.get_pixel(x, y)) - rgb(falloff_zero.get_pixel(x, y))) > 0.025:
				falloff_pixels += 1
	for values: Array in falloff_materials.values():
		values[0].set_shader_parameter("falloff", values[1])
	check(falloff_pixels >= MIN_FIRE_PIXELS, "live flame falloff changes opacity: %d" % falloff_pixels)
	print("RND-38 falloff_pixels=", falloff_pixels)
	# Authored alpha-mixed flame layers remain alpha mixed. Measure additive layers separately.
	hearth = hearth.filter(func(row: Dictionary) -> bool: return row.effect and int(row.dst_blend) == 0)
	check(not hearth.is_empty(), "authored additive hearth layers exist")
	for row: Dictionary in streamer.call("nif_surfaces"):
		RenderingServer.instance_set_visible(row.rid, hearth.any(func(layer: Dictionary) -> bool: return layer.rid == row.rid))
	backdrop_material.set_shader_parameter("backdrop_color", Vector3.ZERO)
	visible_hearth(false)
	dark_empty = await capture("additive_dark_empty")
	visible_hearth(true)
	dark_fire = await capture("additive_dark_fire")
	backdrop_material.set_shader_parameter("backdrop_color", Vector3(0.08, 0.12, 0.16))
	visible_hearth(false)
	light_empty = await capture("additive_light_empty")
	visible_hearth(true)
	light_fire = await capture("additive_light_fire")
	var hue_errors := 0
	var additive_errors := 0
	var fire_pixels := 0
	var backdrop_pixels := 0
	var dim_pixels := 0
	var brightest := 0.0
	for y in range(dark_fire.get_height()):
		for x in range(dark_fire.get_width()):
			var dark := rgb(dark_fire.get_pixel(x, y)) - rgb(dark_empty.get_pixel(x, y))
			var intensity := strength(dark)
			if intensity > 0.025:
				fire_pixels += 1
				brightest = maxf(brightest, intensity)
				if palette_distance(dark) > COLOR_TOLERANCE:
					hue_errors += 1
			var contrast := rgb(light_empty.get_pixel(x, y)) - rgb(dark_empty.get_pixel(x, y))
			if strength(contrast) < 0.06:
				continue
			if intensity > 0.025:
				backdrop_pixels += 1
			var expected := (rgb(dark_fire.get_pixel(x, y)) + contrast).min(Vector3.ONE)
			if strength((expected - rgb(light_fire.get_pixel(x, y))).abs()) > ADDITIVE_TOLERANCE:
				additive_errors += 1
			if intensity > 0.001 and intensity < 0.025:
				dim_pixels += 1
	check(fire_pixels >= MIN_FIRE_PIXELS and brightest > 0.04, "flames brighten the background: %d, %f" % [fire_pixels, brightest])
	check(hue_errors == 0, "all fire hues come from the effects palettes: %d of %d" % [hue_errors, fire_pixels])
	check(backdrop_pixels >= MIN_FIRE_PIXELS, "contrasting backdrop reaches the fire: %d" % backdrop_pixels)
	check(additive_errors == 0, "every hearth pixel transmits the backdrop additively: %d" % additive_errors)
	check(dim_pixels >= MIN_EDGE_PIXELS, "flame edges fade without filling the hull: %d" % dim_pixels)
	print("RND-38 pixels=", fire_pixels, " palette_errors=", hue_errors,
		" additive_errors=", additive_errors, " dim_edges=", dim_pixels)
	backdrop.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " rnd_33_bannered_mare_hearth_capture")
	quit(0 if failures == 0 else 1)
