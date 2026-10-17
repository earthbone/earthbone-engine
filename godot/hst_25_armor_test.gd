extends "res://walkthrough.gd"

## Inventory transactions in the free walk scene, with both player rigs present.
var failures := 0
var checked := false

func _initialize() -> void:
	if OS.get_environment("SOE_EQUIPMENT_EXTERIOR") == "1":
		startup_cell = "WhiterunOrigin"
		startup_radius = 0
	super._initialize()

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _spawn_position(streamer: Node3D) -> Vector3:
	if startup_radius < 0:
		return Vector3(-125.0, -800.0, 120.0)
	var stats: Dictionary = streamer.call("last_stats")
	return Vector3((int(stats.origin_x) + 0.5) * 4096.0, (int(stats.origin_y) + 0.5) * 4096.0, 500.0)

func _process(delta: float) -> bool:
	if loaded and not checked:
		checked = true
		call_deferred("_check_equipment")
	return super._process(delta)

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: HST-25 equipment ", message)

func _steps(count: int) -> void:
	for _step in count:
		player_body.call("advance", 1.0 / 60.0)
		player_arms.call("advance", 1.0 / 60.0)
		await process_frame

func _models(visual: Node3D) -> Array:
	var models := []
	for shape in visual.call("play_path_observation").get("shapes", []):
		var mesh: ArrayMesh = shape.mesh
		models.append([shape.name, mesh.surface_get_array_len(0), mesh.surface_get_array_index_len(0)])
	return models

func _toggle(object: int) -> void:
	_check(player.call("open_inventory_menu"), "inventory did not open")
	_check(player.call("item_menu_activate", {"file": 0, "object": object}, 1, 0), "inventory activation failed %x" % object)
	player.call("close_menu")
	await _steps(2)

func _capture(name: String) -> Image:
	await process_frame
	await RenderingServer.frame_post_draw
	var image := get_root().get_texture().get_image()
	_check(image != null and not image.is_empty(), "empty capture " + name)
	if image != null and not image.is_empty():
		_check(image.save_png("res://.godot/hst_25_equipment_" + name + ".png") == OK, "capture write " + name)
	return image

func _difference(a: Image, b: Image) -> int:
	var changed := 0
	for y in a.get_height():
		for x in a.get_width():
			var first := a.get_pixel(x, y)
			var second := b.get_pixel(x, y)
			if Vector3(first.r - second.r, first.g - second.g, first.b - second.b).length() > 0.05:
				changed += 1
	return changed

func _check_equipment() -> void:
	system_overlay.hide()
	player_body.set("playing", false)
	player_arms.set("playing", false)
	if startup_radius >= 0:
		_check(not str(player.call("console_execute", "set gamehour to 12")).contains("error:"), "exterior fixture could not select noon")
	for object in [0x12e49, 0x12e46, 0x12eb7, 0x12eb6]:
		var result := str(player.call("console_execute", "player.additem %x 1" % object))
		_check(not result.contains("error:"), "fixture additem: " + result)
	await _steps(120)
	player.call("advance", 1.0 / 60.0)
	Input.action_press("Toggle POV")
	player.call("advance", 1.0 / 60.0)
	Input.action_release("Toggle POV")
	player.call("advance", 1.0 / 60.0)
	var bare := [_models(player_body), _models(player_arms)]
	print("HST-25 equipment bare=", bare)
	var before: Image
	if DisplayServer.get_name() != "headless":
		before = await _capture("bare")
	await _toggle(0x12e49)
	await _toggle(0x12e46)
	var armored := [_models(player_body), _models(player_arms)]
	_check(str(armored[1]).to_lower().contains("gauntlet"), "equipped gauntlets left the first person hands missing")
	print("HST-25 equipment armored=", armored)
	_check(bare[0] != armored[0], "armor did not replace third person body")
	_check(bare[1] != armored[1], "gauntlets did not replace first person hands")
	if before != null:
		var dressed := await _capture("armored")
		var pixels := _difference(before, dressed)
		print("HST-25 equipment armor pixels=", pixels)
		_check(pixels >= 100, "armor changed fewer than 100 pixels")
	await _toggle(0x12e46)
	await _toggle(0x12e49)
	_check(bare == [_models(player_body), _models(player_arms)], "unequip did not restore the body and hands")
	await _check_weapons()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " HST-25 inventory equipment")
	quit(0 if failures == 0 else 1)

func _check_weapons() -> void:
	pass
