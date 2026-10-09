extends SceneTree

## PRF-28 light capacity in stock Godot, through server RIDs.
## Warm-up and frame accounting reuse bench.gd. Local Godot RenderingServer
## docs specify shadow flags, the atlas and clustered-element limits.

const Metrics := preload("res://bench_metrics.gd")
const Warmup := preload("res://render_warmup.gd")
const Ladder := preload("res://capacity_curve.gd")
var metrics := Metrics.new()
var curve := Ladder.new()
var world_kind := "synthetic"
var shadows := false
var holder: Node3D
var streamer: Node3D
var camera: Camera3D
var instances: Array[RID] = []
var lights: Array[RID] = []
var geometry: Array[RID] = []
var resources: Array[Resource] = []
var measuring := false
var last_tick := 0
var baseline := true
var baseline_pass := false
var baseline_pixels := 0
var bounds := AABB(Vector3(-16, -1, -16), Vector3(32, 8, 32))
var scene_stats: Dictionary = {}


func _initialize() -> void:
	metrics.fixed_viewport(get_root())
	var error := metrics.configure()
	if not error.is_empty() or DisplayServer.get_name() == "headless" or metrics.soak:
		_fail("windowed non-soak renderer required. " + error)
		return
	world_kind = OS.get_environment("SOE_CAPACITY_WORLD")
	if world_kind.is_empty():
		world_kind = "synthetic"
	var kind := OS.get_environment("SOE_CAPACITY_LIGHTS")
	if kind.is_empty():
		kind = "dynamic"
	if world_kind not in ["synthetic", "whiterun"] or kind not in ["dynamic", "shadow"]:
		_fail("invalid world or light kind")
		return
	shadows = kind == "shadow"
	var start := _count("SOE_CAPACITY_START", 1)
	var maximum := _count("SOE_CAPACITY_MAX", 256)
	error = curve.configure(start, maximum)
	if not error.is_empty():
		_fail(error)
		return
	var clustered := int(ProjectSettings.get_setting("rendering/limits/cluster_builder/max_clustered_elements", 512))
	if RenderingServer.get_current_rendering_method() != "forward_plus" or maximum > clustered:
		_fail("Forward+ required and max must not exceed clustered elements (%d)" % clustered)
		return
	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	Engine.max_fps = 0
	RenderingServer.viewport_set_measure_render_time(get_root().get_viewport_rid(), true)
	get_root().positional_shadow_atlas_size = 4096
	for quadrant in 4:
		get_root().set_positional_shadow_atlas_quadrant_subdiv(quadrant, Viewport.SHADOW_ATLAS_QUADRANT_SUBDIV_256)
	call_deferred("_setup")


func _count(name: String, fallback: int) -> int:
	var value := OS.get_environment(name)
	return fallback if value.is_empty() else int(value) if value.is_valid_int() else -1


func _setup() -> void:
	holder = Node3D.new()
	get_root().add_child(holder)
	if world_kind == "whiterun":
		var data := OS.get_environment("SKYRIM_DATA")
		if data.is_empty():
			_fail("SKYRIM_DATA required for Whiterun")
			return
		streamer = ClassDB.instantiate("CellStreamerNode") as Node3D
		streamer.set("cell_occluders", false)
		holder.add_child(streamer)
		if not streamer.call("load_data", data) or streamer.call("stream_exterior", "WhiterunOrigin", 1) <= 0:
			_fail("Whiterun could not load")
			return
		scene_stats = streamer.call("last_stats")
		if scene_stats.get("meshes_failed", 0) > 0:
			_fail("Whiterun meshes failed")
			return
		bounds = streamer.call("scene_bounds")
		print("CAPACITY_SCENE_BOUNDS " + str(bounds))
		# CONTEXT: Buried placed geometry extends the bounds far below the city.
		var ceiling := bounds.end.y
		bounds.position.y = ceiling - 8.0
		bounds.size.y = 8.0
	camera = Camera3D.new()
	holder.add_child(camera)
	var radius := maxf(Vector2(bounds.size.x, bounds.size.z).length() * 0.5, 20.0)
	var target := bounds.get_center()
	camera.far = radius * 8.0 + 100.0
	camera.position = target + Vector3(0, radius * 0.8, radius * 1.4)
	camera.look_at(target, Vector3.UP)
	camera.make_current()
	var env_node := WorldEnvironment.new()
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.05, 0.05, 0.05)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color.WHITE
	environment.ambient_light_energy = 0.15
	env_node.environment = environment
	holder.add_child(env_node)
	var plane := PlaneMesh.new()
	plane.size = Vector2(bounds.size.x, bounds.size.z)
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(0.5, 0.5, 0.5)
	plane.material = material
	resources.append(plane)
	resources.append(material)
	if world_kind == "synthetic":
		_add_geometry(plane, Transform3D(Basis.IDENTITY, target - Vector3(0, bounds.size.y * 0.5, 0)))
	var cube := BoxMesh.new()
	cube.size = Vector3(0.6, 1.5, 0.6) * (radius / 20.0)
	cube.material = material
	resources.append(cube)
	for n in 64:
		var position := _position(n, 64)
		_add_geometry(cube, Transform3D(Basis.IDENTITY, position - Vector3(0, cube.size.y, 0)))
	await _warm()


func _position(index: int, count: int) -> Vector3:
	var side := int(ceil(sqrt(count)))
	return bounds.position + Vector3((float(index % side) + 0.5) / side * bounds.size.x, bounds.size.y * 0.5, (float(index / side) + 0.5) / side * bounds.size.z)


func _add_geometry(mesh: Mesh, transform: Transform3D) -> void:
	var instance := RenderingServer.instance_create2(mesh.get_rid(), holder.get_world_3d().scenario)
	RenderingServer.instance_set_transform(instance, transform)
	geometry.append(instance)


func _warm() -> void:
	var warmed: Dictionary = await Warmup.prepare()
	if not warmed.error.is_empty():
		_fail(warmed.error)
		return
	metrics.next_window()
	last_tick = 0
	measuring = true


func _build_lights() -> void:
	_clear_lights()
	var radius := maxf(bounds.size.x, bounds.size.z) / maxf(sqrt(curve.load_count), 1.0)
	for n in curve.load_count:
		var light := RenderingServer.omni_light_create()
		RenderingServer.light_set_param(light, RenderingServer.LIGHT_PARAM_RANGE, radius)
		RenderingServer.light_set_param(light, RenderingServer.LIGHT_PARAM_ENERGY, 0.7)
		RenderingServer.light_set_color(light, Color.from_hsv(fmod(n * 0.618, 1.0), 0.4, 1.0))
		RenderingServer.light_set_shadow(light, shadows)
		var instance := RenderingServer.instance_create2(light, holder.get_world_3d().scenario)
		RenderingServer.instance_set_transform(instance, Transform3D(Basis.IDENTITY, _position(n, curve.load_count)))
		instances.append(instance)
		lights.append(light)


func _process(_delta: float) -> bool:
	if not measuring:
		return false
	var now := Time.get_ticks_usec()
	if last_tick != 0:
		var rid := get_root().get_viewport_rid()
		metrics.observe((now - last_tick) / 1000.0, RenderingServer.viewport_get_measured_render_time_gpu(rid), RenderingServer.viewport_get_measured_render_time_cpu(rid))
	last_tick = now
	for n in lights.size():
		var position := _position(n, curve.load_count) + Vector3(0, sin(now / 1000000.0 + n) * 0.5, 0)
		RenderingServer.instance_set_transform(instances[n], Transform3D(Basis.IDENTITY, position))
	if not metrics.ready():
		return false
	measuring = false
	var row := metrics.report({"row": "PRF-28", "world": world_kind, "scenario": "shadow_lights" if shadows else "dynamic_lights", "load": 0 if baseline else curve.load_count, "light_rids": lights.size(), "per_light_nodes": 0, "baseline": baseline, "p99_limit_ms": metrics.p99_limit, "worst_limit_ms": metrics.worst_limit, "scene": scene_stats, "shadow_atlas_size": 4096, "shadow_atlas_maps": 1024})
	_finish_window(row)
	return false


func _finish_window(row: Dictionary) -> void:
	await RenderingServer.frame_post_draw
	var pixels := hash(get_root().get_texture().get_image().get_data())
	if baseline:
		baseline_pixels = pixels
	elif pixels == baseline_pixels:
		_fail("lights did not alter rendered pixels")
		return
	if shadows and not baseline:
		for light in lights:
			RenderingServer.light_set_shadow(light, false)
		await process_frame
		await RenderingServer.frame_post_draw
		await process_frame
		await RenderingServer.frame_post_draw
		var unshadowed := hash(get_root().get_texture().get_image().get_data())
		if unshadowed == pixels:
			_fail("shadow flags did not alter rendered pixels")
			return
		print("CAPACITY_SHADOW_VERIFIED " + str(curve.load_count))
	var capture := OS.get_environment("SOE_CAPACITY_CAPTURE")
	if baseline and not capture.is_empty():
		get_root().get_texture().get_image().save_png(capture)
	if world_kind == "whiterun" and row.primitives <= 768:
		_fail("streamed Whiterun geometry was not drawn")
		return
	if not metrics.rendered_work(row):
		_fail("no rendered geometry")
		return
	var passed := metrics.within_budget(row)
	if baseline:
		baseline = false
		baseline_pass = passed
	else:
		curve.observe(passed)
		if curve.finished:
			var result: Dictionary = curve.result()
			result.merge({"row": "PRF-28", "world": world_kind, "scenario": row.scenario, "baseline_pass": baseline_pass, "adapter": row.adapter, "render_size": row.render_size, "p99_limit_ms": metrics.p99_limit, "worst_limit_ms": metrics.worst_limit})
			print("CAPACITY_RESULT " + JSON.stringify(result))
			_clear_scene()
			quit(0)
			return
	_build_lights()
	await _warm()


func _clear_lights() -> void:
	for rid in instances:
		RenderingServer.free_rid(rid)
	instances.clear()
	for rid in lights:
		RenderingServer.free_rid(rid)
	lights.clear()


func _clear_scene() -> void:
	_clear_lights()
	for rid in geometry:
		RenderingServer.free_rid(rid)
	geometry.clear()
	resources.clear()
	if streamer != null:
		streamer.call("clear")


func _fail(message: String) -> void:
	measuring = false
	_clear_scene()
	printerr("RESULT: ERROR: " + message)
	quit(1)
