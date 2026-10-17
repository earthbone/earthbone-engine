extends "res://walkthrough.gd"

## Exercise every station in the installed crafting warehouse through activation.
var checked := false
var failures := 0

func _initialize() -> void:
	startup_cell = "WarehouseCrafting"
	startup_position = Vector3(900.0, 1500.0, 6620.0)
	super._initialize()

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _process(delta: float) -> bool:
	if loaded and not checked:
		checked = true
		call_deferred("_check_stations")
	return super._process(delta)

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: R4 crafting ", message)

func _steps(count: int) -> void:
	for _step in count:
		player.call("advance", 1.0 / 60.0)
		player_body.call("advance", 1.0 / 60.0)
		player_arms.call("advance", 1.0 / 60.0)
		await process_frame

func _expected(model: String) -> String:
	if "alchemy" in model: return "alchemy"
	if "enchant" in model: return "enchant"
	if "forge" in model: return "blacksmithforge"
	if "anvil" in model: return "blacksmith"
	if "workbench" in model: return "blacksmith"
	if "sharpen" in model: return "blacksmith"
	if "cooking" in model: return "cooking"
	return ""

func _check_stations() -> void:
	system_overlay.hide()
	player_body.set("playing", false)
	player_arms.set("playing", false)
	await _steps(120)
	var tested := 0
	for station in player.call("play_path_references"):
		if station.kind != "FURN": continue
		var model := str(station.model).to_lower()
		var expected := _expected(model)
		if expected.is_empty(): continue
		tested += 1
		_check(player.call("activate_reference", station.file, station.object), model + " activation rejected")
		await _steps(900)
		var clips := str(player_body.call("active_clips")).to_lower()
		print("R4 crafting station=", station, " menu=", player.call("is_menu_open"), " position=", player.call("position"), " clips=", clips)
		_check(expected in clips, model + " has no station idle")
		if DisplayServer.get_name() != "headless":
			await RenderingServer.frame_post_draw
			var capture := get_root().get_texture().get_image()
			_check(capture != null and not capture.is_empty(), model + " capture empty")
			if capture != null and not capture.is_empty():
				_check(capture.save_png("res://.godot/r4_crafting_" + str(station.object) + ".png") == OK, model + " capture write")
		if player.call("is_menu_open"):
			player.call("close_menu")
		else:
			Input.action_press("Forward")
			await _steps(1)
			Input.action_release("Forward")
		await _steps(300)
		clips = str(player_body.call("active_clips")).to_lower()
		_check(not expected in clips, model + " did not exit")
	_check(tested >= 7, "warehouse did not cover the seven requested station kinds")
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 crafting walkthrough stations=", tested, " failures=", failures)
	quit(0 if failures == 0 else 1)
