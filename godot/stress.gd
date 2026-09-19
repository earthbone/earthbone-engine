extends SceneTree

## Isolated host renderer capacity. Uses server RIDs like the engine's bulk
## renderer. This does not exercise NIF decoding, actor skinning or gameplay.

const Metrics := preload("res://bench_metrics.gd")
var metrics := Metrics.new()
var scenario := "geometry"
var load_count := 1
var instances: Array[RID] = []
var resources: Array[Resource] = []
var frame := 0
var last_tick := 0
var root_node: Node3D
var ready_to_measure := false
var setup_ms := 0.0


func _initialize() -> void:
	metrics.fixed_viewport(get_root())
	var error := metrics.configure()
	if not error.is_empty() or DisplayServer.get_name() == "headless":
		printerr("RESULT: ERROR: windowed renderer required. " + error)
		quit(1)
		return
	scenario = OS.get_environment("SOE_BENCH_SCENARIO")
	var raw := OS.get_environment("SOE_BENCH_LOAD")
	if scenario not in ["geometry", "textures"] or not raw.is_valid_int() or int(raw) < 1 or int(raw) > 100000:
		printerr("RESULT: ERROR: invalid scenario or load")
		quit(1)
		return
	load_count = int(raw)
	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	Engine.max_fps = 0
	RenderingServer.viewport_set_measure_render_time(get_root().get_viewport_rid(), true)
	call_deferred("_setup")


func _setup() -> void:
	var began := Time.get_ticks_usec()
	root_node = Node3D.new()
	get_root().add_child(root_node)
	if not _build():
		printerr("RESULT: ERROR: could not build requested instances")
		_clear()
		quit(1)
		return
	var side := float(ceil(sqrt(load_count)))
	var camera := Camera3D.new()
	root_node.add_child(camera)
	camera.position = Vector3(side * 1.5, side * 3.5, side * 1.5)
	camera.far = side * 10.0 + 100.0
	camera.look_at(Vector3(side * 1.5, 0.0, side * 1.5), Vector3.FORWARD)
	camera.make_current()
	setup_ms = (Time.get_ticks_usec() - began) / 1000.0
	ready_to_measure = true


func _build() -> bool:
	var world := root_node.get_world_3d()
	if world == null:
		return false
	var mesh := PlaneMesh.new()
	mesh.size = Vector2(2.5, 2.5)
	mesh.subdivide_width = 15 if scenario == "geometry" else 0
	mesh.subdivide_depth = 15 if scenario == "geometry" else 0
	resources.append(mesh)
	var shared := StandardMaterial3D.new()
	shared.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	resources.append(shared)
	var side := int(ceil(sqrt(load_count)))
	for n in load_count:
		var material := shared
		if scenario == "textures":
			var pixels := Image.create(512, 512, true, Image.FORMAT_RGBA8)
			pixels.fill(Color.from_hsv(fmod(float(n + int(OS.get_environment("SOE_BENCH_SEED"))) * 0.618, 1.0), 0.8, 0.9))
			pixels.generate_mipmaps()
			var texture := ImageTexture.create_from_image(pixels)
			material = StandardMaterial3D.new()
			material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
			material.albedo_texture = texture
			resources.append(material)
		var instance := RenderingServer.instance_create()
		RenderingServer.instance_set_base(instance, mesh.get_rid())
		RenderingServer.instance_set_scenario(instance, world.scenario)
		RenderingServer.instance_geometry_set_material_override(instance, material.get_rid())
		RenderingServer.instance_set_transform(instance, Transform3D(Basis.IDENTITY, Vector3((n % side) * 3.0, 0.0, (n / side) * 3.0)))
		instances.append(instance)
	return instances.size() == load_count


func _clear() -> void:
	for instance in instances:
		RenderingServer.free_rid(instance)
	instances.clear()
	resources.clear()


func _process(_delta: float) -> bool:
	if not ready_to_measure:
		return false
	frame += 1
	var now := Time.get_ticks_usec()
	if frame <= 10:
		last_tick = now
		return false
	var viewport := get_root().get_viewport_rid()
	metrics.observe((now - last_tick) / 1000.0, RenderingServer.viewport_get_measured_render_time_gpu(viewport), RenderingServer.viewport_get_measured_render_time_cpu(viewport))
	last_tick = now
	if not metrics.ready():
		return false
	var reporting_started := Time.get_ticks_usec()
	var row := metrics.report({"load": load_count, "setup_ms": setup_ms, "scenario": scenario, "instances": instances.size(), "triangles_per_instance": 512 if scenario == "geometry" else 2, "unique_textures": load_count if scenario == "textures" else 0})
	if not metrics.rendered_work(row) or instances.size() != load_count or row["primitives"] < load_count * (512 if scenario == "geometry" else 2):
		printerr("RESULT: ERROR: requested geometry was not rendered")
		_clear()
		quit(1)
		ready_to_measure = false
	elif not metrics.within_budget(row):
		printerr("RESULT: FAIL: frame budget exceeded")
		_clear()
		quit(2)
		ready_to_measure = false
	elif not metrics.soak or metrics.expired():
		print("RESULT: PASS")
		_clear()
		quit(0)
		ready_to_measure = false
	else:
		metrics.next_window()
		last_tick += Time.get_ticks_usec() - reporting_started
		_clear()
		if not _build():
			printerr("RESULT: ERROR: scene rebuild failed")
			quit(1)
			ready_to_measure = false
	return false
