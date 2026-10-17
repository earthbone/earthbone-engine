extends "res://walkthrough.gd"

## Exercise Ctrl through the free walk controller and inspect authored clips.
var failures := 0
var checked := false

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _spawn_position(_streamer: Node3D) -> Vector3:
	return Vector3(-125.0, -800.0, 120.0)

func _process(delta: float) -> bool:
	if loaded and not checked:
		checked = true
		call_deferred("_check_sneak")
	return super._process(delta)

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: R4 sneak ", message)

func _steps(count: int) -> void:
	for _step in count:
		player.call("advance", 1.0 / 60.0)
		player_body.call("advance", 1.0 / 60.0)
		player_arms.call("advance", 1.0 / 60.0)
		await process_frame

func _press(action: String) -> void:
	Input.action_press(action)
	await _steps(1)
	Input.action_release(action)
	await _steps(2)

func _capture(label: String) -> void:
	if DisplayServer.get_name() == "headless":
		return
	await process_frame
	await RenderingServer.frame_post_draw
	var frame := get_root().get_texture().get_image()
	_check(frame != null and not frame.is_empty(), label + " capture empty")
	if frame != null and not frame.is_empty():
		_check(frame.save_png("res://.godot/r4_sneak_" + label + ".png") == OK, label + " capture write")

func _clips(label: String, required: String) -> void:
	for visual in [player_body, player_arms]:
		var clips := str(visual.call("active_clips")).to_lower()
		print("R4 sneak ", label, " ", visual.name, " clips=", clips)
		_check(required in clips, label + " " + str(visual.name) + " missing " + required)

func _check_sneak() -> void:
	system_overlay.hide()
	player_body.set("playing", false)
	player_arms.set("playing", false)
	await _steps(120)
	_check(player.call("is_on_ground"), "player did not settle")
	await _press("Toggle POV")
	var camera: Camera3D = get_root().get_camera_3d()
	var third_height := camera.global_position.y
	await _capture("standing")
	await _press("Sneak")
	await _steps(60)
	_check(player.call("is_sneaking"), "Ctrl did not toggle sneak")
	_clips("idle", "sneak")
	_check(camera.global_position.y < third_height - 0.1, "third person camera did not lower")
	await _capture("idle")
	Input.action_press("Forward")
	Input.action_press("Walk")
	await _steps(12)
	_clips("walk", "sneakwalk")
	Input.action_release("Walk")
	await _steps(12)
	_clips("run", "sneakrun")
	Input.action_release("Forward")
	await _steps(2)
	await _press("Toggle POV")
	var first_crouched := camera.global_position.y
	await _press("Sneak")
	await _steps(60)
	_check(not player.call("is_sneaking"), "second Ctrl did not stand")
	_check(camera.global_position.y > first_crouched + 0.1, "first person camera did not restore height")
	print("R4 sneak standing clips=", player_body.call("active_clips"))
	_check(not "sneak" in str(player_body.call("active_clips")).to_lower(), "standing kept sneak clip")
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 sneak walkthrough failures=", failures)
	quit(0 if failures == 0 else 1)
