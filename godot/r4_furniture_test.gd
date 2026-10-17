extends "res://walkthrough.gd"

## Activate an installed chair through the actual free walk host path.
var checked := false
var failures := 0

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _spawn_position(_streamer: Node3D) -> Vector3:
	return Vector3(-125.0, -800.0, 120.0)

func _process(delta: float) -> bool:
	if loaded and not checked:
		checked = true
		call_deferred("_check_chair")
	return super._process(delta)

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: R4 furniture ", message)

func _steps(count: int) -> void:
	for _step in count:
		player.call("advance", 1.0 / 60.0)
		player_body.call("advance", 1.0 / 60.0)
		player_arms.call("advance", 1.0 / 60.0)
		await process_frame

func _check_chair() -> void:
	system_overlay.hide()
	player_body.set("playing", false)
	player_arms.set("playing", false)
	await _steps(120)
	Input.action_press("Toggle POV")
	await _steps(1)
	Input.action_release("Toggle POV")
	var chairs: Array[Dictionary] = []
	for reference in player.call("play_path_references"):
		if reference.kind == "FURN" and "chair" in str(reference.model).to_lower():
			chairs.append(reference)
	_check(not chairs.is_empty(), "no installed chair in Bannered Mare")
	if not chairs.is_empty():
		var chair := chairs[0]
		_check(player.call("activate_reference", chair.file, chair.object), "chair activation rejected")
		await _steps(180)
		var clips := str(player_body.call("active_clips")).to_lower()
		print("R4 furniture chair=", chair, " position=", player.call("position"), " clips=", clips)
		_check("chair" in clips or "sit" in clips, "activation did not play a seated clip")
		if DisplayServer.get_name() != "headless":
			await RenderingServer.frame_post_draw
			var capture := get_root().get_texture().get_image()
			_check(capture != null and not capture.is_empty(), "seated capture empty")
			if capture != null and not capture.is_empty():
				_check(capture.save_png("res://.godot/r4_furniture_seated.png") == OK, "seated capture write")
		Input.action_press("Forward")
		await _steps(1)
		Input.action_release("Forward")
		await _steps(180)
		clips = str(player_body.call("active_clips")).to_lower()
		print("R4 furniture exit clips=", clips)
		_check(not "chair" in clips and not "sit" in clips, "movement did not leave furniture")
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 furniture walkthrough failures=", failures)
	quit(0 if failures == 0 else 1)
