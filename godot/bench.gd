extends SceneTree

## Frame-time benchmark. Streams a cell (or an exterior block) through the
## real data path, then runs at least 600 frames and five seconds and prints
## percentiles (p50, p99, p99.9, worst) of the wall frame time, the
## viewport's measured GPU and CPU render time, and the render setup time.
## Headless runs have no renderer, so only the frame time means anything
## there.
##
##   SKYRIM_DATA=<Data> godot --headless --path godot --script bench.gd
##   SKYRIM_DATA=<Data> godot --path godot --script bench.gd
##
## Frame time is the wall clock between two _process calls. The
## Performance.TIME_PROCESS monitor the first version printed is not a
## per-frame number (it read a constant 56 to 113 ms across every frame of a
## scene that draws in under a millisecond), which is how docs/performance.md
## came to call the Bannered Mare a 56 ms frame on 2026-09-17.
##
## Knobs, all environment variables:
##   SOE_BENCH_CELL      editor id, default WhiterunBanneredMare
##   SOE_BENCH_RADIUS    set to stream the exterior block of that radius
##                       around SOE_BENCH_CELL instead of one interior
##   SOE_BENCH_BATCH     0 for one instance per surface, default 1
##   SOE_BENCH_CAPTURE   PNG path, windowed only, saved after the warm-up

const FRAMES := 600
const SECONDS := 5
const WARM_UP := 10

var frame := 0
var frame_ms: Array[float] = []
var last_tick := 0
var measure_start := 0
var render_setup_ms: Array[float] = []
var gpu_ms: Array[float] = []
var render_cpu_ms: Array[float] = []
var streamed := false
var done := false
var scene_holder: Node3D
var streamer_node: Node3D
var capture_path := ""


func _initialize() -> void:
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
	var batch := OS.get_environment("SOE_BENCH_BATCH") != "0"
	capture_path = OS.get_environment("SOE_BENCH_CAPTURE")

	var scene_root := Node3D.new()
	get_root().add_child(scene_root)
	var streamer := ClassDB.instantiate("CellStreamerNode") as Node3D
	if streamer == null:
		printerr("RESULT: FAIL: could not instantiate CellStreamerNode")
		quit(1)
		return
	streamer.set("batch_surfaces", batch)
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
	print("godot_display_driver %s driver" % DisplayServer.get_name())
	print("godot_load_data %.3f s" % ((opened - started) / 1_000_000.0))
	print("godot_stream_%s %.3f s" % [label, (finished - opened) / 1_000_000.0])
	print("godot_instances %d instances" % built)
	if built <= 0:
		printerr("RESULT: FAIL: nothing built for %s" % label)
		quit(1)
		return
	var stats: Dictionary = streamer.call("last_stats")
	for key in ["cells", "references", "statics", "meshes_parsed", "meshes_failed", "triangles", "surfaces", "materials", "largest_batch", "land_cells"]:
		print("godot_%s %d count" % [key, stats[key]])
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
	streamed = true


## The camera frames the bounds the streamer reports, since the geometry is
## RenderingServer instances and not mesh nodes the tree could be walked for.
func _setup_camera() -> void:
	var merged: AABB = streamer_node.call("scene_bounds")
	var camera := Camera3D.new()
	scene_holder.add_child(camera)
	var centre := merged.get_center()
	var radius: float = maxf(merged.size.length() * 0.5, 1.0)
	camera.far = radius * 8.0 + 100.0
	camera.position = centre + Vector3(0.0, radius * 0.6, radius * 1.4)
	camera.look_at(centre, Vector3.UP)
	camera.make_current()
	var light := DirectionalLight3D.new()
	light.rotation_degrees = Vector3(-50.0, 30.0, 0.0)
	scene_holder.add_child(light)
	var env := WorldEnvironment.new()
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.55, 0.65, 0.8)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color(0.6, 0.6, 0.65)
	environment.ambient_light_energy = 0.8
	env.environment = environment
	scene_holder.add_child(env)
	print("godot_scene_extent %.1f m" % merged.size.length())


func _percentile(sorted: Array[float], fraction: float) -> float:
	if sorted.is_empty():
		return 0.0
	var index := int(floor(fraction * (sorted.size() - 1)))
	return sorted[clampi(index, 0, sorted.size() - 1)]


func _report(name: String, values: Array[float]) -> void:
	var sorted := values.duplicate()
	sorted.sort()
	print("godot_%s_p50 %.3f ms" % [name, _percentile(sorted, 0.5)])
	print("godot_%s_p99 %.3f ms" % [name, _percentile(sorted, 0.99)])
	print("godot_%s_p999 %.3f ms" % [name, _percentile(sorted, 0.999)])
	print("godot_%s_worst %.3f ms" % [name, _percentile(sorted, 1.0)])


func _capture() -> void:
	if capture_path.is_empty() or DisplayServer.get_name() == "headless":
		return
	var image := get_root().get_texture().get_image()
	var err := image.save_png(capture_path)
	print("godot_capture %s %s" % [capture_path, "ok" if err == OK else error_string(err)])


func _process(_delta: float) -> bool:
	if not streamed or done:
		return false
	frame += 1
	if frame == 1:
		_setup_camera()
	if frame == WARM_UP:
		_capture()
	var now := Time.get_ticks_usec()
	if frame <= WARM_UP:
		last_tick = now
		return false
	# Wall time between two _process calls: the whole frame, presentation
	# included. Performance.TIME_PROCESS is sampled once a second, so it
	# cannot give percentiles.
	frame_ms.append((now - last_tick) / 1000.0)
	last_tick = now
	if measure_start == 0:
		measure_start = now
	render_setup_ms.append(RenderingServer.get_frame_setup_time_cpu() * 1000.0)
	if DisplayServer.get_name() != "headless":
		var rid := get_root().get_viewport_rid()
		gpu_ms.append(RenderingServer.viewport_get_measured_render_time_gpu(rid))
		render_cpu_ms.append(RenderingServer.viewport_get_measured_render_time_cpu(rid))
	# At least FRAMES frames and at least SECONDS of wall time, so a scene
	# that draws in a fraction of a millisecond still gets a sample long
	# enough for the p99.9 and the worst hitch to mean something.
	if frame < FRAMES + WARM_UP or now - measure_start < SECONDS * 1_000_000:
		return false
	done = true
	print("godot_frames %d frames" % frame_ms.size())
	print("godot_measured_wall %.3f s" % ((now - measure_start) / 1_000_000.0))
	print("godot_frame_mean %.3f ms" % ((now - measure_start) / 1000.0 / frame_ms.size()))
	_report("frame", frame_ms)
	_report("render_setup_cpu", render_setup_ms)
	_report("viewport_gpu", gpu_ms)
	_report("viewport_cpu", render_cpu_ms)
	print("godot_fps %.0f fps" % Performance.get_monitor(Performance.TIME_FPS))
	print("godot_objects_drawn %d objects" % int(Performance.get_monitor(Performance.RENDER_TOTAL_OBJECTS_IN_FRAME)))
	print("godot_primitives_drawn %d primitives" % int(Performance.get_monitor(Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME)))
	print("godot_draw_calls %d calls" % int(Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME)))
	print("godot_memory_static %.1f MB" % (Performance.get_monitor(Performance.MEMORY_STATIC) / 1_048_576.0))
	print("godot_video_memory %.1f MB" % (Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED) / 1_048_576.0))
	print("godot_texture_memory %.1f MB" % (Performance.get_monitor(Performance.RENDER_TEXTURE_MEM_USED) / 1_048_576.0))
	print("godot_buffer_memory %.1f MB" % (Performance.get_monitor(Performance.RENDER_BUFFER_MEM_USED) / 1_048_576.0))
	print("RESULT: PASS")
	quit(0)
	return true
