extends SceneTree

## Runs a JSON scripted-player route in a window and records each step.
const DEFAULT_ROUTE := "res://ply_04_route.json"
const DEFAULT_OUTPUT := "/tmp/soe-ply-04"

var streamer: Node3D
var player: Node3D
var output_dir := ""
var route_name := ""
var report := {"route": "", "route_result": "running", "steps": []}
var failures := 0


func _initialize() -> void:
	call_deferred("_run_route")


func _run_route() -> void:
	var route_path := OS.get_environment("SOE_PLY_04_ROUTE")
	if route_path.is_empty():
		route_path = DEFAULT_ROUTE
	output_dir = OS.get_environment("SOE_PLY_04_OUTPUT_DIR")
	if output_dir.is_empty():
		output_dir = DEFAULT_OUTPUT
	var output_absolute := ProjectSettings.globalize_path(output_dir)
	if DirAccess.make_dir_recursive_absolute(output_absolute) != OK:
		_fail("cannot create output directory " + output_absolute)
		_finish()
		return

	var route_file := FileAccess.open(route_path, FileAccess.READ)
	if route_file == null:
		_fail("cannot open route " + route_path)
		_finish()
		return
	var route_value: Variant = JSON.parse_string(route_file.get_as_text())
	if not route_value is Dictionary:
		_fail("route root must be an object")
		_finish()
		return
	var route: Dictionary = route_value
	route_name = str(route.get("name", route_path.get_file().get_basename()))
	report["route"] = route_name
	var steps: Variant = route.get("steps", null)
	if not steps is Array or steps.is_empty():
		_fail("route steps must be a non-empty array")
		_finish()
		return
	if DisplayServer.get_name() == "headless":
		_fail("a windowed renderer is required")
		_finish()
		return
	var skyrim_data := OS.get_environment("SKYRIM_DATA")
	if skyrim_data.is_empty():
		_fail("SKYRIM_DATA is not set")
		_finish()
		return
	if not ClassDB.class_exists("CellStreamerNode") or not ClassDB.class_exists("PlayerNode"):
		_fail("the Godot host extension did not register its classes")
		_finish()
		return

	print("ply_04 loading route=", route_name)
	streamer = ClassDB.instantiate("CellStreamerNode") as Node3D
	player = ClassDB.instantiate("PlayerNode") as Node3D
	if streamer == null or player == null:
		_fail("could not instantiate the host nodes")
		_finish()
		return
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(streamer)
	root.add_child(player)
	print("ply_04 loading installed data")
	if not streamer.call("load_data", skyrim_data):
		_fail("host could not load SKYRIM_DATA")
		_finish()
		return
	if not player.call("use_streamer_data", streamer):
		_fail("player could not share the host data")
		_finish()
		return
	var start: Variant = route.get("start", {})
	if not start is Dictionary:
		_fail("route start must be an object")
		_finish()
		return
	var start_cell := str(start.get("cell", ""))
	var position: Variant = start.get("position", [0.0, 0.0, 0.0])
	if start_cell.is_empty() or not position is Array or position.size() != 3:
		_fail("route start requires cell and three position values")
		_finish()
		return
	print("ply_04 entering cell=", start_cell)
	if int(player.call("enter_cell", start_cell, float(position[0]), float(position[1]), float(position[2]))) < 0:
		_fail("player could not enter cell " + start_cell)
		_finish()
		return
	if not player.call("start_gameplay"):
		_fail("player could not start gameplay")
		_finish()
		return

	var checkpoint_names := {}
	var checkpoint_count := 0
	for index in range(steps.size()):
		var raw_step: Variant = steps[index]
		if not raw_step is Dictionary:
			_fail("step %d must be an object" % index)
			break
		var step: Dictionary = raw_step
		var command: Variant = step.get("command", null)
		if not command is Dictionary:
			_fail("step %d requires a command object" % index)
			break
		var frames := int(step.get("frames", 2))
		if frames < 1 or frames > 6000:
			_fail("step %d frames must be from 1 to 6000" % index)
			break
		var checkpoint := str(step.get("checkpoint", ""))
		if not checkpoint.is_empty():
			if not _valid_checkpoint(checkpoint) or checkpoint_names.has(checkpoint):
				_fail("step %d has an invalid or repeated checkpoint name" % index)
				break
			checkpoint_names[checkpoint] = true
			checkpoint_count += 1

		var runtime_command := _runtime_command(command)
		if not player.call("queue_player_input", runtime_command):
			_fail("step %d command was rejected" % index)
			break
		if not player.call("step_player_input"):
			_fail("step %d did not execute" % index)
			break
		var worst_ms := 0.0
		var last_tick := Time.get_ticks_usec()
		for _frame in range(frames):
			await process_frame
			var now := Time.get_ticks_usec()
			worst_ms = maxf(worst_ms, float(now - last_tick) / 1000.0)
			last_tick = now
		var entry := {
			"step": index,
			"action": str(command.get("action", "")),
			"frames": frames,
			"worst_frame_ms": worst_ms,
			"checkpoint": checkpoint,
		}
		if not checkpoint.is_empty():
			var capture_path := output_absolute.path_join(checkpoint + ".png")
			var capture_error := root.get_texture().get_image().save_png(capture_path)
			if capture_error != OK:
				_fail("step %d could not save checkpoint capture: %s" % [index, error_string(capture_error)])
			entry["capture"] = capture_path
		(report["steps"] as Array).append(entry)
		print("ply_04 step=%d action=%s worst_frame_ms=%.3f checkpoint=%s" % [index, entry["action"], worst_ms, checkpoint])
		if failures > 0:
			break
	if checkpoint_count == 0:
		_fail("route has no checkpoint captures")
	if failures == 0:
		report["route_result"] = "pass"
	_finish()


func _runtime_command(command: Dictionary) -> Dictionary:
	var converted := command.duplicate(true)
	for key in ["offset", "rotation"]:
		if converted.has(key) and converted[key] is Array and converted[key].size() == 3:
			var values: Array = converted[key]
			converted[key] = Vector3(float(values[0]), float(values[1]), float(values[2]))
	return converted


func _valid_checkpoint(name: String) -> bool:
	if name.is_empty() or name == "." or name == "..":
		return false
	for character in name:
		if not (character.is_valid_identifier() or character == "-" or character == "."):
			return false
	return true


func _fail(message: String) -> void:
	failures += 1
	printerr("FAIL ply_04: " + message)


func _finish() -> void:
	if failures > 0:
		report["route_result"] = "fail"
	else:
		report["route_result"] = "pass"
	if not output_dir.is_empty():
		var absolute := ProjectSettings.globalize_path(output_dir)
		if DirAccess.make_dir_recursive_absolute(absolute) == OK:
			var log := FileAccess.open(absolute.path_join("frame_times.json"), FileAccess.WRITE)
			if log != null:
				log.store_string(JSON.stringify(report, "\t") + "\n")
			else:
				failures += 1
				report["route_result"] = "fail"
				printerr("FAIL ply_04: cannot write frame_times.json")
	print("ply_04 route_result=", report["route_result"])
	quit(0 if failures == 0 else 1)
