extends SceneTree

## Frame-time benchmark. Streams a cell (or an exterior block) through the
## real data path, then runs at least 600 frames and five seconds and prints
## percentiles (p50, p99, p99.9, worst) of the wall frame time, the
## viewport's measured GPU and CPU render time.
## Requires a windowed renderer. Headless invocations fail without claiming
## rendering performance. Defaults: p99 <= 16.7 ms and worst <= 33 ms.
##
##   SKYRIM_DATA=<Data> godot --path godot --script bench.gd
##   Add -- --cold-starts N to launch N fresh windowed processes sequentially.
##
## Frame time is the wall clock between two _process calls. The
## Performance.TIME_PROCESS monitor the first version printed is not a
## per-frame number (it read a constant 56 to 113 ms across every frame of a
## scene that draws in under a millisecond), which is how docs/performance.md
## came to call the Bannered Mare a 56 ms frame on 2026-09-17.
##
## Knobs, all environment variables:
##   SOE_BENCH_P99_MS / SOE_BENCH_WORST_MS positive frame budgets
##   SOE_BENCH_SECONDS  measurement duration, default 5
##   SOE_BENCH_SOAK     1 to restream after each measured window
##   SOE_BENCH_CELL      editor id, default WhiterunBanneredMare
##   SOE_BENCH_RADIUS    set to stream the exterior block of that radius
##                       around SOE_BENCH_CELL instead of one interior
##   SOE_BENCH_BATCH     0 for one instance per surface, default 1
##   SOE_BENCH_CAPTURE   PNG path, windowed only, saved after the warm-up

const Metrics := preload("res://bench_metrics.gd")
const Warmup := preload("res://render_warmup.gd")
const Runs := preload("res://bench_runs.gd")
var metrics := Metrics.new()
var bench_cell := ""
var bench_radius := ""
var bench_extra: Dictionary = {}
var run_number := 1
var pipelines_at_measurement: Array[int] = []

var frame := 0
var last_tick := 0
var streamed := false
var done := false
var scene_holder: Node3D
var streamer_node: Node3D
var capture_path := ""


func _initialize() -> void:
	var repetition := Runs.configure(OS.get_cmdline_user_args())
	if not repetition.error.is_empty():
		printerr("RESULT: ERROR: " + repetition.error)
		quit(1)
		return
	if repetition.launcher:
		if OS.get_environment("SOE_BENCH_SOAK") == "1":
			printerr("RESULT: ERROR: cold starts cannot be combined with a soak")
			quit(1)
			return
		call_deferred("_cold_starts", repetition.runs)
		return
	run_number = repetition.run
	metrics.fixed_viewport(get_root())
	var error := metrics.configure()
	if not error.is_empty() or DisplayServer.get_name() == "headless":
		printerr("RESULT: ERROR: windowed renderer required. " + error)
		quit(1)
		return
	var skyrim_data := OS.get_environment("SKYRIM_DATA")
	if skyrim_data.is_empty():
		printerr("RESULT: FAIL: SKYRIM_DATA is not set")
		quit(1)
		return
	if not ClassDB.class_exists("CellStreamerNode"):
		printerr("RESULT: FAIL: CellStreamerNode is not registered, the extension did not load")
		quit(1)
		return
	var cell := OS.get_environment("SOE_BENCH_CELL")
	if cell.is_empty():
		cell = "WhiterunBanneredMare"
	var radius_text := OS.get_environment("SOE_BENCH_RADIUS")
	if not radius_text.is_empty() and (not radius_text.is_valid_int() or int(radius_text) < 0 or int(radius_text) > 128):
		printerr("RESULT: ERROR: radius must be 0 to 128")
		quit(1)
		return
	bench_cell = cell
	bench_radius = radius_text
	var batch := OS.get_environment("SOE_BENCH_BATCH") != "0"
	var occluders := OS.get_environment("SOE_BENCH_OCCLUDERS") != "0"
	capture_path = OS.get_environment("SOE_BENCH_CAPTURE")

	ProjectSettings.set_setting("rendering/occlusion_culling/use_occlusion_culling", occluders)
	call_deferred("_setup_scene", skyrim_data, cell, radius_text, batch, occluders)


func _cold_starts(runs: int) -> void:
	quit(Runs.launch(ProjectSettings.globalize_path("res://"), runs))


func _setup_scene(skyrim_data: String, cell: String, radius_text: String, batch: bool, occluders: bool) -> void:
	var loading_cover := Warmup.cover(get_root())
	var scene_root := Node3D.new()
	get_root().add_child(scene_root)
	var streamer := ClassDB.instantiate("CellStreamerNode") as Node3D
	if streamer == null:
		printerr("RESULT: FAIL: could not instantiate CellStreamerNode")
		quit(1)
		return
	streamer.set("batch_surfaces", batch)
	streamer.set("cell_occluders", occluders)
	scene_root.add_child(streamer)
	var started := Time.get_ticks_usec()
	if not streamer.call("load_data", skyrim_data):
		printerr("RESULT: FAIL: load_data could not open %s" % skyrim_data)
		quit(1)
		return
	var opened := Time.get_ticks_usec()
	var built: int
	var label: String
	if radius_text.is_empty():
		built = streamer.call("stream_cell", cell)
		label = cell
	else:
		built = streamer.call("stream_exterior", cell, int(radius_text))
		label = "%s_r%s" % [cell, radius_text]
	var finished := Time.get_ticks_usec()
	print("godot_scene %s scene" % label)
	print("godot_batch %s bool" % str(batch))
	print("godot_cell_occluders %s bool" % str(occluders))
	print("godot_display_driver %s driver" % DisplayServer.get_name())
	print("godot_load_data %.3f s" % ((opened - started) / 1_000_000.0))
	print("godot_stream_%s %.3f s" % [label, (finished - opened) / 1_000_000.0])
	print("godot_instances %d instances" % built)
	if built <= 0:
		printerr("RESULT: FAIL: nothing built for %s" % label)
		quit(1)
		return
	if streamer.call("surface_material_count") != built:
		printerr("RESULT: ERROR: streamed mesh is missing a surface material")
		quit(1)
		return
	var stats: Dictionary = streamer.call("last_stats")
	if stats["meshes_failed"] > 0:
		printerr("RESULT: ERROR: some scene meshes failed to load")
		quit(1)
		return
	for key in ["cells", "references", "statics", "meshes_parsed", "meshes_failed", "triangles", "surfaces", "materials", "largest_batch", "land_cells"]:
		print("godot_%s %d count" % [key, stats[key]])
	bench_extra = stats.duplicate()
	bench_extra["load"] = int(radius_text) if not radius_text.is_empty() else 1
	bench_extra["setup_ms"] = (finished - started) / 1000.0
	print("godot_world %s worldspace" % stats["world"])
	print("godot_cell_list %s cells" % stats["cell_list"])
	print("godot_headless %s bool" % str(DisplayServer.get_name() == "headless"))
	# Frame time should measure the work, not the monitor, so vsync and the
	# frame cap go off. A camera on the scene's bounds makes the renderer draw
	# every surface instead of culling an empty view.
	if DisplayServer.get_name() != "headless":
		DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
		RenderingServer.viewport_set_measure_render_time(get_root().get_viewport_rid(), true)
	Engine.max_fps = 0
	scene_holder = scene_root
	streamer_node = streamer
	_apply_cell_lighting(scene_holder, streamer.call("cell_lighting", cell))
	_setup_camera()
	var sky := preload("res://record_sky.gd").new()
	scene_root.add_child(sky)
	if not sky.use_streamer(streamer, cell):
		printerr("RESULT: FAIL: could not initialize the record sky")
		quit(1)
		return
	var warmed: Dictionary = await Warmup.prepare()
	if not warmed.error.is_empty():
		printerr("RESULT: ERROR: " + warmed.error)
		quit(1)
		return
	bench_extra.merge(warmed)
	bench_extra["run"] = run_number
	print("godot_load_render %s" % JSON.stringify(warmed))
	loading_cover.queue_free()
	await process_frame
	await RenderingServer.frame_post_draw
	_capture()
	await process_frame
	pipelines_at_measurement = Warmup.snapshot()
	last_tick = 0
	streamed = true


## The camera frames the bounds the streamer reports, since the geometry is
## RenderingServer instances and not mesh nodes the tree could be walked for.
func _setup_camera() -> void:
	var merged: AABB = streamer_node.call("scene_bounds")
	var camera := Camera3D.new()
	scene_holder.add_child(camera)
	var centre := merged.get_center()
	var radius: float = maxf(Vector2(merged.size.x, merged.size.z).length() * 0.5, 1.0)
	var target := Vector3(centre.x, merged.position.y + merged.size.y, centre.z)
	camera.far = radius * 8.0 + 100.0
	camera.position = target + Vector3(0.0, radius * 0.6, radius * 1.4)
	camera.look_at(target, Vector3.UP)
	camera.make_current()
	print("godot_scene_extent %.1f m" % (radius * 2.0))


func _apply_cell_lighting(parent: Node3D, values: Dictionary) -> void:
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.05, 0.05, 0.07)
	if values.is_empty():
		var world := WorldEnvironment.new()
		world.environment = environment
		parent.add_child(world)
		return
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = values["ambient"]
	environment.ambient_light_energy = 1.0
	environment.fog_enabled = values["fog_far"] > values["fog_near"]
	environment.fog_light_color = values["fog_near_color"]
	environment.fog_depth_begin = maxf(values["fog_near"], 0.0)
	var fog_end: float = values["fog_far"]
	if values["fog_clip_distance"] > 0.0:
		fog_end = minf(fog_end, values["fog_clip_distance"])
	environment.fog_depth_end = maxf(fog_end, environment.fog_depth_begin + 0.01)
	environment.fog_density = clampf(values["fog_power"] * values["fog_max"], 0.0, 1.0)
	var world := WorldEnvironment.new()
	world.environment = environment
	parent.add_child(world)
	var light := DirectionalLight3D.new()
	light.name = "CellDirectionalAmbient"
	light.light_color = values["directional"]
	light.light_energy = clampf(values["directional_fade"], 0.0, 1.0)
	light.rotation = Vector3(deg_to_rad(values["rotation_xy"]), 0.0, deg_to_rad(values["rotation_z"]))
	light.distance_fade_enabled = values["light_fade_end"] > values["light_fade_begin"]
	light.distance_fade_begin = values["light_fade_begin"]
	light.distance_fade_length = maxf(values["light_fade_end"] - values["light_fade_begin"], 0.0)
	light.shadow_enabled = false
	parent.add_child(light)
	var rotations := [
		Vector3(0.0, -PI / 2.0, 0.0), Vector3(0.0, PI / 2.0, 0.0),
		Vector3(PI / 2.0, 0.0, 0.0), Vector3(-PI / 2.0, 0.0, 0.0),
		Vector3(0.0, PI, 0.0), Vector3.ZERO,
	]
	var ambient_directions: Array = values["ambient_directions"]
	for index in ambient_directions.size():
		var ambient := DirectionalLight3D.new()
		ambient.name = "CellAmbientDirection%d" % index
		ambient.light_color = ambient_directions[index]
		ambient.light_energy = 1.0 / 6.0
		ambient.shadow_enabled = false
		ambient.rotation = rotations[index]
		parent.add_child(ambient)

func _capture() -> void:
	if capture_path.is_empty() or DisplayServer.get_name() == "headless":
		return
	var image := get_root().get_texture().get_image()
	var err := image.save_png(capture_path)
	print("godot_capture %s %s" % [capture_path, "ok" if err == OK else error_string(err)])


func _process(_delta: float) -> bool:
	if not streamed or done:
		return false
	var now := Time.get_ticks_usec()
	if last_tick == 0:
		last_tick = now
		return false
	frame += 1
	var rid := get_root().get_viewport_rid()
	metrics.observe((now - last_tick) / 1000.0, RenderingServer.viewport_get_measured_render_time_gpu(rid), RenderingServer.viewport_get_measured_render_time_cpu(rid), frame)
	last_tick = now
	if not metrics.ready():
		return false
	var reporting_started := Time.get_ticks_usec()
	var pipeline_delta: Array[int] = []
	var current := Warmup.snapshot()
	for index in current.size():
		pipeline_delta.append(current[index] - pipelines_at_measurement[index])
	bench_extra["pipelines_during_measurement"] = Warmup.named(pipeline_delta)
	var row := metrics.report(bench_extra)
	pipelines_at_measurement = current
	for key in ["p50_ms", "p99_ms", "p999_ms", "worst_ms"]:
		print("godot_frame_%s %.3f ms" % [key.trim_suffix("_ms"), row[key]])
	print("godot_run %d worst %.3f ms frame %d" % [run_number, row.worst_ms, row.worst_frame])
	if not metrics.rendered_work(row):
		printerr("RESULT: ERROR: scene produced no rendered geometry")
		done = true
		quit(1)
	elif not metrics.within_budget(row):
		printerr("RESULT: FAIL: frame budget exceeded")
		done = true
		quit(2)
	elif not metrics.soak or metrics.expired():
		print("RESULT: PASS")
		done = true
		quit(0)
	else:
		metrics.next_window()
		last_tick += Time.get_ticks_usec() - reporting_started
		streamer_node.call("clear")
		var built: int
		if bench_radius.is_empty():
			built = streamer_node.call("stream_cell", bench_cell)
		else:
			built = streamer_node.call("stream_exterior", bench_cell, int(bench_radius))
		if built <= 0 or streamer_node.call("instance_count") != built:
			printerr("RESULT: ERROR: restream failed or retained old instances")
			done = true
			quit(1)
	return done
