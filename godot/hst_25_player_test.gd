extends "res://walkthrough.gd"

## HST-25 uses the actual walkthrough startup and its player appearances.
var failures := 0
var checked := false
var contacts := 0

func _contact(name: String, _left: bool, _pose: Transform3D) -> void:
	if name == "HitFrame":
		contacts += 1

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _spawn_position(_streamer: Node3D) -> Vector3:
	return Vector3(-125.0, -800.0, 120.0)

func _process(delta: float) -> bool:
	if loaded and not checked:
		checked = true
		call_deferred("_check_player")
	return super._process(delta)

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: HST-25 ", message)

func _steps(count: int) -> void:
	for _step in count:
		player.call("advance", 1.0 / 60.0)
		await process_frame

func _pixels(a: Image, b: Image) -> int:
	var changed := 0
	for y in a.get_height():
		for x in a.get_width():
			if a.get_pixel(x, y) != b.get_pixel(x, y):
				changed += 1
	return changed

func _capture(name: String) -> Image:
	await process_frame
	await RenderingServer.frame_post_draw
	var image := get_root().get_texture().get_image()
	_check(image != null and not image.is_empty(), "empty capture " + name)
	if image != null and not image.is_empty():
		_check(image.save_png("res://.godot/hst_25_" + name + ".png") == OK, "capture write " + name)
	return image

func _check_player() -> void:
	system_overlay.hide()
	_check(player_body.get_parent() == player, "body is not under the player")
	_check(player_arms.get_parent() == player, "arms are not under the player")
	for visual in [player_body, player_arms]:
		var stats: Dictionary = visual.call("stats")
		_check(stats.get("skeleton_bones", 0) > 0, "empty skeleton")
		_check(stats.get("shapes", 0) > 0, "empty mesh set")
	player_body.connect("weapon_contact", _contact)
	player_body.set("playing", false)
	player_arms.set("playing", false)
	await _steps(120)
	player_body.call("advance", 1.0 / 60.0)
	player_arms.call("advance", 1.0 / 60.0)
	Input.action_press("Toggle POV")
	await _steps(1)
	Input.action_release("Toggle POV")
	await _steps(1)
	_check(player.call("is_third_person"), "F action did not switch to third person")
	if DisplayServer.get_name() != "headless":
		var shown: Image = await _capture("body")
		player_body.call("set_first_person_geometry_visible", false)
		var hidden: Image = await _capture("hidden")
		if shown != null and hidden != null:
			var difference := _pixels(shown, hidden)
			print("HST-25 body pixels=", difference)
			_check(difference >= 100, "body contributed fewer than 100 pixels")
		player_body.call("set_first_person_geometry_visible", true)
	var idle: Array = player_body.call("uploaded_skin_poses")
	Input.action_press("Forward")
	await _steps(10)
	Input.action_release("Forward")
	await _steps(2)
	for _step in 20:
		player_body.call("advance", 1.0 / 60.0)
	var walk: Array = player_body.call("uploaded_skin_poses")
	_check(idle != walk, "walking did not advance the skeleton")
	print("HST-25 states ", player_body.call("active_states"))
	print("HST-25 clips ", player_body.call("active_clips"))
	await _check_hands()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " HST-25 walkthrough body")
	quit(0 if failures == 0 else 1)

func _check_hands() -> void:
	_check(player.call("equip_melee_weapon", 0, 0x12EB7, false), "iron sword equip failed")
	_check(player.call("toggle_weapon_ready"), "ready weapon did not draw")
	for _step in 90:
		player_body.call("advance", 1.0 / 60.0)
		player_arms.call("advance", 1.0 / 60.0)
		await process_frame
	for view in ["third", "first"]:
		if view == "first":
			Input.action_press("Toggle POV")
			await _steps(1)
			Input.action_release("Toggle POV")
			await _steps(1)
			_check(not player.call("is_third_person"), "F did not return to first person")
		_check(player.call("equip_melee_weapon", 0, 0x12EB7, false), "sword re-equip failed")
		player_body.call("advance", 0.0)
		player_arms.call("advance", 0.0)
		var visual: Node3D = player_body if view == "third" else player_arms
		_check(visual.call("stats").get("hand_shapes", 0) > 0, view + " sword has no mesh")
		if DisplayServer.get_name() != "headless":
			var armed: Image = await _capture(view + "_sword")
			_check(player.call("unequip_player_hand", false), "sword unequip failed")
			player_body.call("advance", 0.0)
			player_arms.call("advance", 0.0)
			var empty: Image = await _capture(view + "_empty")
			if armed != null and empty != null:
				var pixels := _pixels(armed, empty)
				print("HST-25 ", view, " weapon pixels=", pixels)
				_check(pixels >= 25, view + " sword contributed fewer than 25 pixels")
		_check(player.call("equip_melee_weapon", 0, 0x12EB7, false), "sword attack equip failed")
		player_body.call("advance", 0.0)
		player_arms.call("advance", 0.0)
		var before: Array = visual.call("uploaded_skin_poses")
		var markers := contacts
		var attack_clip := false
		_check(player.call("attack_with_hand", false, false), "right attack was rejected")
		for _step in 120:
			player_body.call("advance", 1.0 / 60.0)
			player_arms.call("advance", 1.0 / 60.0)
			for clip in visual.call("active_clips"):
				if "attack" in str(clip).to_lower():
					attack_clip = true
			await process_frame
		_check(before != visual.call("uploaded_skin_poses"), view + " attack did not advance skeleton")
		_check(attack_clip, view + " authored attack clip did not play")
		_check(contacts > markers, view + " body did not submit an authored contact")
		print("HST-25 ", view, " attack clips ", visual.call("active_clips"))
		player.call("cancel_melee_swing")
