extends SceneTree

## Frame-time benchmark. Streams the Bannered Mare through the real data
## path, then runs 300 frames and prints the median process, physics and
## render setup times. Headless runs have no renderer, so their render
## number is zero and only the process time means anything.
##
##   SKYRIM_DATA=<Data> godot --headless --path godot --script bench.gd
##   SKYRIM_DATA=<Data> godot --path godot --script bench.gd

const FRAMES := 300
const CELL := "WhiterunBanneredMare"

var frame := 0
var process_ms: Array[float] = []
var physics_ms: Array[float] = []
var render_setup_ms: Array[float] = []
var streamed := false
var done := false


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
	var scene_root := Node3D.new()
	get_root().add_child(scene_root)
	var streamer := ClassDB.instantiate("CellStreamerNode") as Node3D
	if streamer == null:
		printerr("RESULT: FAIL: could not instantiate CellStreamerNode")
		quit(1)
		return
	scene_root.add_child(streamer)
	var started := Time.get_ticks_usec()
	if not streamer.call("load_data", skyrim_data):
		printerr("RESULT: FAIL: load_data could not open %s" % skyrim_data)
		quit(1)
		return
	var opened := Time.get_ticks_usec()
	var built: int = streamer.call("stream_cell", CELL)
	var finished := Time.get_ticks_usec()
	print("godot_load_data %.3f s" % ((opened - started) / 1_000_000.0))
	print("godot_stream_cell_%s %.3f s" % [CELL, (finished - opened) / 1_000_000.0])
	print("godot_mesh_nodes %d nodes" % built)
	if built <= 0:
		printerr("RESULT: FAIL: stream_cell built no meshes for %s" % CELL)
		quit(1)
		return
	print("godot_headless %s bool" % str(DisplayServer.get_name() == "headless"))
	# Frame time should measure the work, not the monitor, so vsync and the
	# frame cap go off. A camera on the cell's bounds makes the renderer draw
	# every surface instead of culling an empty view.
	if DisplayServer.get_name() != "headless":
		DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	Engine.max_fps = 0
	scene_holder = scene_root
	streamed = true


var scene_holder: Node3D


## The mesh nodes are only inside the tree from the first frame on, so the
## camera is placed then, on the merged bounds of every mesh instance.
func _setup_camera() -> void:
	var merged := AABB()
	var first := true
	for child in scene_holder.find_children("*", "MeshInstance3D", true, false):
		var mesh_instance := child as MeshInstance3D
		var box := mesh_instance.global_transform * mesh_instance.get_aabb()
		if first:
			merged = box
			first = false
		else:
			merged = merged.merge(box)
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
	print("godot_scene_extent %.1f units" % merged.size.length())


func _median(values: Array[float]) -> float:
	if values.is_empty():
		return 0.0
	var sorted := values.duplicate()
	sorted.sort()
	return sorted[sorted.size() / 2]


func _process(_delta: float) -> bool:
	if not streamed or done:
		return false
	frame += 1
	if frame == 1:
		_setup_camera()
	if frame <= 10:
		return false
	process_ms.append(Performance.get_monitor(Performance.TIME_PROCESS) * 1000.0)
	physics_ms.append(Performance.get_monitor(Performance.TIME_PHYSICS_PROCESS) * 1000.0)
	render_setup_ms.append(RenderingServer.get_frame_setup_time_cpu() * 1000.0)
	if frame < FRAMES + 10:
		return false
	done = true
	print("godot_frames %d frames" % FRAMES)
	print("godot_process_median %.3f ms" % _median(process_ms))
	print("godot_physics_median %.3f ms" % _median(physics_ms))
	print("godot_render_setup_cpu_median %.3f ms" % _median(render_setup_ms))
	print("godot_objects_drawn %d objects" % int(Performance.get_monitor(Performance.RENDER_TOTAL_OBJECTS_IN_FRAME)))
	print("godot_primitives_drawn %d primitives" % int(Performance.get_monitor(Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME)))
	print("RESULT: PASS")
	quit(0)
	return true
