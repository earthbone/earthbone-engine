extends Node3D

## WTHR/CLMT presentation. The sampler reads the gameplay World when bound.
const SKY_SHADER := preload("res://record_sky.gdshader")
const PHASES := ["full", "three_wan", "half_wan", "one_wan", "new", "one_wax", "half_wax", "three_wax"]
const UNITS_PER_METRE := 70.0

var records = ClassDB.instantiate("SkyRecords")
var cell := ""
var environment := Environment.new()
var material := ShaderMaterial.new()
var sun := DirectionalLight3D.new()
var directional := DirectionalLight3D.new()
var ambient_directions: Array[DirectionalLight3D] = []
var last_frame: Dictionary = {}
var elapsed := 0.0
var cloud_paths: Array[String] = []
var cloud_array: Texture2DArray
var night_model := ""
var phase := -1
var fog_material := ShaderMaterial.new()
var fog_pass := MeshInstance3D.new()

func _ready() -> void:
	material.shader = SKY_SHADER
	var dome := Sky.new()
	dome.sky_material = material
	dome.process_mode = Sky.PROCESS_MODE_INCREMENTAL
	dome.radiance_size = Sky.RADIANCE_SIZE_32
	environment.sky = dome
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.reflected_light_source = Environment.REFLECTION_SOURCE_SKY
	environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
	var world_environment := WorldEnvironment.new()
	world_environment.environment = environment
	add_child(world_environment)
	add_child(sun)
	sun.name = "WeatherSun"
	sun.shadow_enabled = true
	sun.visible = false
	directional.name = "CellDirectional"
	directional.visible = false
	add_child(directional)
	var axes := [Vector3.RIGHT, Vector3.LEFT, Vector3.FORWARD, Vector3.BACK, Vector3.UP, Vector3.DOWN]
	for index in axes.size():
		var light := DirectionalLight3D.new()
		light.name = "CellAmbientDirection%d" % index
		var up := Vector3.FORWARD if absf(axes[index].y) > 0.99 else Vector3.UP
		light.basis = Basis.looking_at(-axes[index], up)
		light.light_energy = 1.0 / 6.0
		light.visible = false
		ambient_directions.append(light)
		add_child(light)
	fog_material.shader = preload("res://record_fog.gdshader")
	fog_material.render_priority = 127
	var quad := QuadMesh.new()
	quad.size = Vector2(2.0, 2.0)
	fog_pass.mesh = quad
	fog_pass.material_override = fog_material
	fog_pass.extra_cull_margin = 1000000.0
	fog_pass.ignore_occlusion_culling = true
	fog_pass.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(fog_pass)

func use_streamer(streamer: Node3D, cell_name: String, hour := 12.0, days := 0.0) -> bool:
	cell = cell_name
	if not records.use_streamer(streamer):
		return false
	_reset_textures()
	records.set_time(hour, days)
	return refresh()

func use_player(player: Node3D) -> bool:
	_reset_textures()
	return records.use_player(player) and refresh()

func _reset_textures() -> void:
	cloud_paths.clear()
	cloud_array = null
	material.set_shader_parameter("cloud_count", 0)
	material.set_shader_parameter("cloud_textures", null)
	material.set_shader_parameter("stars_enabled", false)
	material.set_shader_parameter("stars_texture", null)
	material.set_shader_parameter("masser_texture", null)
	material.set_shader_parameter("secunda_texture", null)
	material.set_shader_parameter("sun_texture", null)
	material.set_shader_parameter("glare_texture", null)
	night_model = ""
	phase = -1

func _process(delta: float) -> void:
	elapsed += delta
	records.advance(delta)
	refresh()

func refresh() -> bool:
	last_frame = records.frame(cell)
	if last_frame.has("error"):
		push_error(last_frame.error)
		return false
	if last_frame.is_empty():
		return true
	var f := last_frame
	var interior: bool = f.has("rotation")
	directional.visible = false
	for light in ambient_directions:
		light.visible = false
	sun.visible = false
	environment.ambient_light_color = f.get("ambient", Color.BLACK)
	environment.ambient_light_energy = 1.0
	if interior:
		directional.light_color = f.sunlight
		directional.light_energy = maxf(0.0, f.fade)
		directional.rotation_degrees = Vector3(-f.rotation.y, f.rotation.x, 0.0)
		directional.visible = directional.light_energy > 0.0
		for index in ambient_directions.size():
			var light := ambient_directions[index]
			light.light_color = f.ambient_directions[index]
			light.visible = true
	elif f.weather_sky:
		sun.visible = f.sun_direction.y > 0.0
		sun.light_color = f.sunlight
		sun.light_energy = 1.0
		var axis := Vector3.FORWARD if absf(f.sun_direction.y) > 0.99 else Vector3.UP
		sun.basis = Basis.looking_at(-f.sun_direction, axis)
	if f.has("fog"):
		_apply_fog(f)
	else:
		fog_pass.visible = false
	if not f.weather_sky:
		environment.background_mode = Environment.BG_COLOR
		environment.background_color = Color.BLACK
		return true
	environment.background_mode = Environment.BG_SKY if f.sky_visible else Environment.BG_COLOR
	environment.background_color = Color.BLACK
	for key in ["upper", "lower", "horizon", "sun_direction", "sun_color", "sun_glare", "night_alpha", "stars_color", "masser_direction", "secunda_direction"]:
		material.set_shader_parameter(key, f[key])
	material.set_shader_parameter("elapsed", elapsed)
	material.set_shader_parameter("masser_enabled", f.masser)
	material.set_shader_parameter("secunda_enabled", f.secunda)
	if phase != f.phase:
		phase = f.phase
		for moon in ["masser", "secunda"]:
			material.set_shader_parameter(moon + "_texture", records.texture("sky/" + moon + "_" + PHASES[phase] + ".dds"))
	for key in ["sun_texture", "glare_texture"]:
		material.set_shader_parameter(key, records.texture(f[key]) if not f[key].is_empty() else null)
	material.set_shader_parameter("sun_enabled", not f.sun_texture.is_empty())
	if night_model != f.night_model:
		night_model = f.night_model
		var texture_path: String = records.night_texture(night_model)
		material.set_shader_parameter("stars_texture", records.texture(texture_path) if not texture_path.is_empty() else null)
		material.set_shader_parameter("stars_enabled", not texture_path.is_empty())
	return _clouds(f.clouds)

func _apply_fog(frame: Dictionary) -> void:
	var near: float = frame.fog.x / UNITS_PER_METRE
	var far: float = frame.fog.y / UNITS_PER_METRE
	if frame.get("fog_clip_distance", 0.0) > 0.0:
		far = minf(far, frame.fog_clip_distance / UNITS_PER_METRE)
	environment.fog_enabled = false
	environment.fog_mode = Environment.FOG_MODE_DEPTH
	environment.fog_depth_begin = near
	environment.fog_depth_end = maxf(near + 0.01, far)
	environment.fog_depth_curve = maxf(0.01, frame.fog.z)
	environment.fog_light_color = frame.fog_color
	environment.fog_light_energy = 1.0
	environment.fog_density = clampf(frame.fog.w, 0.0, 1.0)
	environment.fog_sky_affect = 0.0
	fog_pass.visible = far > 0.0 and frame.fog.w > 0.0
	fog_material.set_shader_parameter("near_color", frame.fog_color)
	fog_material.set_shader_parameter("far_color", frame.fog_far_color)
	fog_material.set_shader_parameter("fog_near", near)
	fog_material.set_shader_parameter("fog_far", far)
	fog_material.set_shader_parameter("fog_power", frame.fog.z)
	fog_material.set_shader_parameter("fog_max", clampf(frame.fog.w, 0.0, 1.0))

func _clouds(clouds: Array) -> bool:
	var paths: Array[String] = []
	for cloud in clouds:
		if not paths.has(cloud.texture):
			paths.append(cloud.texture)
	if paths != cloud_paths:
		var images: Array[Image] = []
		for path in paths:
			var texture: Texture2D = records.texture(path)
			if texture == null:
				push_error("Missing sky cloud texture: " + path)
				return false
			var image := texture.get_image()
			if image.is_compressed() and image.decompress() != OK:
				return false
			image.convert(Image.FORMAT_RGBA8)
			image.clear_mipmaps()
			image.resize(512, 512, Image.INTERPOLATE_LANCZOS)
			images.append(image)
		if not images.is_empty():
			cloud_array = Texture2DArray.new()
			if cloud_array.create_from_images(images) != OK:
				return false
			material.set_shader_parameter("cloud_textures", cloud_array)
		cloud_paths = paths
	if clouds.size() > 256:
		push_error("Sky transition exceeds 256 cloud contributions")
		return false
	var colors := PackedColorArray()
	var velocities := PackedVector2Array()
	var layers := PackedFloat32Array()
	for cloud in clouds:
		var color: Color = cloud.color
		color.a = cloud.alpha
		colors.append(color)
		velocities.append(cloud.speed)
		layers.append(float(paths.find(cloud.texture)))
	colors.resize(256)
	velocities.resize(256)
	layers.resize(256)
	material.set_shader_parameter("cloud_count", clouds.size())
	material.set_shader_parameter("cloud_colors", colors)
	material.set_shader_parameter("cloud_speeds", velocities)
	material.set_shader_parameter("cloud_layers", layers)
	return true
