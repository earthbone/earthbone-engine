extends "res://walkthrough.gd"

## A low tick rate exposes presentation gaps on the ordinary player path.
const SAMPLE_SECONDS := 1.5
const MIN_RENDER_SAMPLES := 60
const MAX_STILL_FRACTION := 0.1
const MAX_SPEED_ERROR_FRACTION := 0.25
var failures := 0
var results := {}


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("R4_MOTION FAIL: ", message)


func _sample(label: String) -> void:
	Engine.physics_ticks_per_second = 10
	Engine.max_fps = 120
	if label == "inn":
		player.call("set_facing", PI, 0.0)
	else:
		player.call("set_facing", 3.0 * PI / 2.0, 0.0)
	player.set("auto_step", true)
	var settle := Time.get_ticks_msec() + 3_000
	while not player.call("is_on_ground") and Time.get_ticks_msec() < settle:
		await process_frame
	_check(player.call("is_on_ground"), label + " starts on collision ground")
	Input.action_press("Forward")
	var warm := Time.get_ticks_msec() + (600 if label == "inn" else 2_000)
	while Time.get_ticks_msec() < warm:
		await process_frame
	await create_timer(0.0).timeout
	var camera := root.get_camera_3d()
	var previous := camera.global_position
	var previous_usec := Time.get_ticks_usec()
	var started := previous_usec
	var samples: Array[Dictionary] = []
	while Time.get_ticks_usec() - started < SAMPLE_SECONDS * 1e6 or samples.size() < MIN_RENDER_SAMPLES:
		await process_frame
		await create_timer(0.0).timeout
		var now := Time.get_ticks_usec()
		var current := camera.global_position
		var distance := Vector2(current.x - previous.x, current.z - previous.z).length()
		samples.append({"dt": (now - previous_usec) / 1e6, "distance": distance, "fraction": Engine.get_physics_interpolation_fraction(), "feet": player.call("position"), "camera": current})
		previous = current
		previous_usec = now
	Input.action_release("Forward")
	player.set("auto_step", false)
	var total_distance := 0.0
	var total_time := 0.0
	var still := 0
	for sample in samples:
		total_distance += sample.distance
		total_time += sample.dt
		if sample.distance < 0.00001:
			still += 1
	var speed := total_distance / total_time
	var error_time := 0.0
	for sample in samples:
		if absf(sample.distance / sample.dt - speed) > speed * MAX_SPEED_ERROR_FRACTION:
			error_time += sample.dt
	results[label] = {"samples": samples, "still_fraction": float(still) / samples.size(), "speed_error_fraction": error_time / total_time, "speed": speed}
	print("R4_MOTION ", label, " samples=", samples.size(), " still=", results[label].still_fraction, " speed_error=", results[label].speed_error_fraction, " speed=", speed)
	_check(samples.size() >= MIN_RENDER_SAMPLES, label + " has enough render samples")
	_check(total_distance > 0.5, label + " moved through the scene")
	_check(results[label].still_fraction < MAX_STILL_FRACTION, label + " camera moves between physics ticks")
	_check(results[label].speed_error_fraction < MAX_SPEED_ERROR_FRACTION, label + " camera speed follows render time")


func _start() -> void:
	Engine.max_fps = 120
	await super._start()
	_check(loaded, "walkthrough loaded")
	if not loaded:
		quit(1)
		return
	await _sample("inn")
	var crossed := false
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			crossed = player.call("activate_reference", door.file, door.object)
			break
	_check(crossed, "Bannered Mare exit activated")
	player.set("auto_step", true)
	var deadline := Time.get_ticks_msec() + 120_000
	while (player.call("budget_runtime_status").world_editor_id != "WhiterunWorld" or player.call("is_location_preparing")) and Time.get_ticks_msec() < deadline:
		await process_frame
	_check(not player.call("is_location_preparing"), "exterior preparation finished")
	_check(player.call("budget_runtime_status").world_editor_id == "WhiterunWorld", "entered Whiterun")
	await _sample("exterior")
	var path := OS.get_environment("SOE_R4_MOTION_REPORT")
	if not path.is_empty():
		var output := FileAccess.open(path, FileAccess.WRITE)
		_check(output != null, "report opened")
		if output != null:
			output.store_string(JSON.stringify(results, "\t"))
	print("R4_MOTION ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
