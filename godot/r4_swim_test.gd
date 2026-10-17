extends "res://walkthrough.gd"

## Recorded deep-water points exercise the free walk controller and both rigs.
var checked := false
var failures := 0
var water_height := 0.0
var fixture_name := "testL"

func _initialize() -> void:
	var requested := OS.get_environment("SOE_SWIM_CASE")
	if not requested.is_empty():
		fixture_name = requested
	var fixtures: Dictionary = JSON.parse_string(FileAccess.get_file_as_string("/tmp/r4-water-fixtures.json"))
	var fixture: Dictionary = fixtures[fixture_name]
	startup_cell = fixture.cell
	startup_radius = int(fixture.radius)
	startup_position = Vector3(fixture.position[0], fixture.position[1], fixture.position[2])
	water_height = float(fixture.water_height)
	super._initialize()

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _process(delta: float) -> bool:
	if loaded and not checked:
		checked = true
		call_deferred("_check_swim")
	return super._process(delta)

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: R4 swim ", fixture_name, " ", message)

func _steps(count: int) -> void:
	for _step in count:
		player.call("advance", 1.0 / 60.0)
		player_body.call("advance", 1.0 / 60.0)
		player_arms.call("advance", 1.0 / 60.0)
		await process_frame

func _capture(label: String) -> void:
	if DisplayServer.get_name() == "headless":
		return
	await RenderingServer.frame_post_draw
	var frame := get_root().get_texture().get_image()
	_check(frame != null and not frame.is_empty(), label + " capture empty")
	if frame != null and not frame.is_empty():
		_check(frame.save_png("res://.godot/r4_swim_" + fixture_name + "_" + label + ".png") == OK, label + " capture write")

func _clips(label: String, stroke: bool) -> void:
	for visual in [player_body, player_arms]:
		var clips := str(visual.call("active_clips")).to_lower()
		print("R4 swim ", fixture_name, " ", label, " ", visual.name, " position=", player.call("position"), " water=", water_height, " clips=", clips)
		_check("swim" in clips, label + " " + str(visual.name) + " has no swim clip")
		if stroke:
			if visual == player_body:
				_check("forward" in clips, label + " " + str(visual.name) + " has no stroke clip")
			else:
				_check("swimidle" in clips, label + " first person lacks its authored locomotion clip")

func _check_swim() -> void:
	system_overlay.hide()
	player_body.set("playing", false)
	player_arms.set("playing", false)
	Input.action_press("Toggle POV")
	await _steps(1)
	Input.action_release("Toggle POV")
	await _steps(180)
	_clips("idle", false)
	var position: Vector3 = player.call("position")
	_check(position.z > water_height - 140.0 and position.z < water_height - 20.0, "buoyancy did not hold the body below the surface")
	await _capture("idle")
	Input.action_press("Forward")
	await _steps(60)
	_clips("stroke", true)
	Input.action_release("Forward")
	await _capture("stroke")
	player.call("set_facing", 0.0, -0.8)
	position = player.call("position")
	Input.action_press("Forward")
	await _steps(60)
	Input.action_release("Forward")
	var submerged: Vector3 = player.call("position")
	_check(submerged.z < position.z - 20.0, "looking down and swimming did not dive")
	player.call("set_facing", 0.0, 0.8)
	Input.action_press("Forward")
	await _steps(120)
	Input.action_release("Forward")
	var surfaced: Vector3 = player.call("position")
	_check(surfaced.z > submerged.z + 20.0, "looking up and swimming did not surface")
	Input.action_press("Toggle POV")
	await _steps(1)
	Input.action_release("Toggle POV")
	await _capture("first_person")
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 swim ", fixture_name, " failures=", failures)
	quit(0 if failures == 0 else 1)
