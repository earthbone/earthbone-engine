extends SceneTree

const Diagnostics := preload("res://load_diagnostics.gd")
const Warmup := preload("res://render_warmup.gd")
var failures := 0


func _initialize() -> void:
	call_deferred("prf_29_windowed_startup_whiterun_upload_and_pipeline_observations")


func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-29 FAIL: ", message)


func prf_29_windowed_startup_whiterun_upload_and_pipeline_observations() -> void:
	if OS.get_environment("SKYRIM_DATA").is_empty():
		print("prf_29_windowed_startup_whiterun_upload_and_pipeline_observations: SKIP SKYRIM_DATA")
		quit(0)
		return
	if DisplayServer.get_name() == "headless":
		printerr("PRF-29 rendering check requires a window")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	var diagnostic := Diagnostics.new()
	diagnostic.streamer = streamer
	get_root().add_child(diagnostic)
	var camera := Camera3D.new()
	camera.position = Vector3(0, 10, 20)
	get_root().add_child(camera)
	camera.make_current()
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-50, 30, 0)
	get_root().add_child(sun)
	check(streamer.call("begin_load_data", OS.get_environment("SKYRIM_DATA")), "background startup failed")
	var state: int = streamer.call("poll_load_data")
	while state == 0:
		await process_frame
		state = streamer.call("poll_load_data")
	check(state == 1, "startup failed")
	if state != 1:
		quit(1)
		return
	check(streamer.call("stream_exterior", "WhiterunOrigin", 0) > 0, "Whiterun upload failed")
	var bounds: AABB = streamer.call("scene_bounds")
	var centre := bounds.get_center()
	var radius := maxf(Vector2(bounds.size.x, bounds.size.z).length() * 0.5, 1.0)
	var target := Vector3(centre.x, bounds.position.y + bounds.size.y, centre.z)
	camera.far = radius * 8.0 + 100.0
	camera.position = target + Vector3(0, radius * 0.6, radius * 1.4)
	camera.look_at(target, Vector3.UP)
	for _frame in 6:
		await RenderingServer.frame_post_draw
	RenderingServer.force_sync()
	check(Performance.get_monitor(Performance.RENDER_TOTAL_OBJECTS_IN_FRAME) > 0, "Whiterun was not drawn")
	var capture := OS.get_environment("SOE_PRF_29_CAPTURE")
	if not capture.is_empty():
		check(get_root().get_texture().get_image().save_png(capture) == OK, "render capture failed")
	var dump: String = streamer.call("load_timing_dump")
	for name in ["startup", "plugin_parse", "form_table", "archive_index", "mesh_parse", "mesh_build", "texture_prepare", "cell_upload"]:
		check(dump.contains("phase=" + name), "missing " + name)
	for cause in ["io_wait", "parse", "decode", "cpu_build", "gpu_upload", "worker_queue"]:
		check(dump.contains("cause=" + cause), "missing " + cause)
	check(dump.contains("object=\"base:"), "missing base-form ownership")
	check(dump.contains("slowest=") and dump.contains("over_budget="), "missing budget attribution")
	check(diagnostic.summary().contains("PIPELINE_COUNTS"), "missing pipeline sources")
	check(Warmup.snapshot().reduce(func(total, value): return total + value, 0) > 0, "no compiled pipelines observed")
	diagnostic.write_dump()
	var text := FileAccess.get_file_as_string("user://soe-loads.txt")
	check(text.contains("LOAD_PHASE") and text.contains("PIPELINE_COUNTS"), "dump lost phase or compilation evidence")
	var trace = JSON.parse_string(FileAccess.get_file_as_string("user://soe-loads.json"))
	check(trace is Dictionary and trace.has("traceEvents"), "Chrome export is invalid JSON")
	streamer.call("clear")
	diagnostic.free()
	streamer.free()
	print("prf_29_windowed_startup_whiterun_upload_and_pipeline_observations: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
