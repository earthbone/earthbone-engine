extends RefCounted

const RecordSky := preload("res://record_sky.gd")
const UNITS_PER_METRE := 70.0
var failed := false

func check(value: bool, message: String) -> void:
	if not value:
		failed = true
		printerr("RND-21 FAIL: ", message)

func controller(root: Node) -> Node3D:
	var found: Node3D
	for node in root.find_children("*", "Node3D", true, false):
		if node.get_script() == RecordSky:
			check(found == null, "only one record lighting controller")
			found = node
	check(found != null, "production script attaches record lighting")
	if found == null:
		return null
	var environments := root.find_children("*", "WorldEnvironment", true, false)
	check(environments.size() == 1 and environments[0].get_parent() == found, "controller owns the only WorldEnvironment")
	check(found.get_world_3d().environment == found.environment, "record Environment is active on World3D")
	var lights := root.find_children("*", "DirectionalLight3D", true, false)
	check(lights.size() == 8, "one sun and seven interior lights")
	for light in lights:
		check(light.get_parent() == found, "no script-owned or stale directional lights")
	return found

func interior(sky: Node3D, expected: Dictionary) -> void:
	check(sky.refresh(), "refresh interior")
	check(not expected.is_empty(), "installed CELL resolves lighting")
	if expected.is_empty():
		return
	check(not sky.sun.visible, "ordinary interior never receives the weather sun")
	check(sky.environment.ambient_light_color == expected.ambient, "XCLL or LGTM ambient reaches the active Environment")
	check(sky.directional.light_color == expected.directional, "authored directional color")
	check(is_equal_approx(sky.directional.light_energy, maxf(0.0, expected.directional_fade)), "authored directional fade")
	check(sky.directional.rotation_degrees.is_equal_approx(Vector3(-expected.rotation_z, expected.rotation_xy, 0.0)), "authored directional rotation")
	var axes := [Vector3.RIGHT, Vector3.LEFT, Vector3.FORWARD, Vector3.BACK, Vector3.UP, Vector3.DOWN]
	for index in range(6):
		check(sky.ambient_directions[index].basis.z.is_equal_approx(axes[index]), "Skyrim ambient axis maps to Godot axis %d" % index)
		check(sky.ambient_directions[index].visible and sky.ambient_directions[index].light_color == expected.ambient_directions[index], "authored ambient axis %d" % index)
	var far: float = expected.fog_far
	if expected.fog_clip_distance > 0.0:
		far = minf(far, expected.fog_clip_distance)
	check(is_equal_approx(sky.fog_material.get_shader_parameter("fog_near"), expected.fog_near / UNITS_PER_METRE), "interior fog near")
	check(is_equal_approx(sky.fog_material.get_shader_parameter("fog_far"), far / UNITS_PER_METRE), "interior fog far and clipping")
	check(sky.fog_material.get_shader_parameter("near_color") == expected.fog_near_color, "interior fog near color")
	check(sky.fog_material.get_shader_parameter("far_color") == expected.fog_far_color, "interior fog far color")
	check(is_equal_approx(sky.fog_material.get_shader_parameter("fog_power"), expected.fog_power), "interior fog power")
	check(is_equal_approx(sky.fog_material.get_shader_parameter("fog_max"), clampf(expected.fog_max, 0.0, 1.0)), "interior fog maximum")
	check(sky.last_frame.light_fade == Vector2(expected.light_fade_begin, expected.light_fade_end), "inherited light fade distances")

func exterior(sky: Node3D) -> void:
	check(sky.refresh(), "refresh exterior")
	var frame: Dictionary = sky.last_frame
	check(frame.get("weather_sky", false) and not frame.has("rotation"), "exterior samples weather lighting")
	if not frame.has("sun_direction"):
		check(false, "weather has a sun direction")
		return
	check(sky.sun.visible == (frame.sun_direction.y > 0.0), "sun visibility follows weather time")
	check(sky.sun.light_color == frame.sunlight, "sunlight color follows WTHR")
	check(sky.sun.basis.z.is_equal_approx(frame.sun_direction.normalized()), "sun direction follows weather time")
	check(sky.environment.ambient_light_color == frame.ambient, "exterior ambient follows WTHR")
	check(not sky.directional.visible, "interior directional light disabled outside")
	for light in sky.ambient_directions:
		check(not light.visible, "interior ambient lights disabled outside")
