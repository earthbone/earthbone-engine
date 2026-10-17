extends "res://hst_25_armor_test.gd"

## Round 4: the full iron kit (cuirass, helmet, gauntlets, boots, sword, shield)
## draws in first and third person, and the first person hands stay visible with
## gauntlets on. Captures go to godot/.godot/r4_iron_*.png.
const KIT := [0x12e49, 0x12e4d, 0x12e46, 0x12e4b, 0x12eb7, 0x12eb6]

func _shot(name: String) -> Image:
	await process_frame
	await RenderingServer.frame_post_draw
	var image := get_root().get_texture().get_image()
	_check(image != null and not image.is_empty(), "empty capture " + name)
	if image != null and not image.is_empty():
		_check(image.save_png("res://.godot/r4_iron_" + name + ".png") == OK, "capture write " + name)
	return image

func _visible_shapes(visual: Node3D) -> Array:
	var rows := []
	for shape in visual.call("play_path_observation").get("shapes", []):
		var mesh: ArrayMesh = shape.mesh
		rows.append("%s:%d" % [shape.name, mesh.surface_get_array_len(0)])
	return rows

func _view(first: bool) -> void:
	if player.call("is_third_person") == first:
		Input.action_press("Toggle POV")
		player.call("advance", 1.0 / 60.0)
		Input.action_release("Toggle POV")
		player.call("advance", 0.0)
	_check(player.call("is_third_person") != first, "view not selected")
	await _steps(30)

func _check_equipment() -> void:
	system_overlay.hide()
	player_body.set("playing", false)
	player_arms.set("playing", false)
	for object in KIT:
		var result := str(player.call("console_execute", "player.additem %x 1" % object))
		_check(not result.contains("error:"), "fixture additem: " + result)
	await _steps(60)
	await _view(true)
	var bare := await _shot("first_bare")
	var bare_models := [_visible_shapes(player_body), _visible_shapes(player_arms)]
	for object in KIT:
		await _toggle(object)
	var worn := [_visible_shapes(player_body), _visible_shapes(player_arms)]
	print("R4_IRON bare third=", bare_models[0], " first=", bare_models[1])
	print("R4_IRON worn third=", worn[0], " first=", worn[1])
	var first_text := str(worn[1]).to_lower()
	var third_text := str(worn[0]).to_lower()
	_check(first_text.contains("gauntlet"), "first person gauntlets missing")
	for part in ["cuirass", "helmet", "boots", "gauntlet"]:
		_check(third_text.contains(part) or part == "cuirass" and third_text.contains("armor"), "third person %s missing" % part)
	await _steps(30)
	var dressed := await _shot("first_sheathed")
	print("R4_IRON first sheathed pixel change vs bare=", _difference(bare, dressed))
	_check(player.call("toggle_weapon_ready"), "draw failed")
	await _steps(120)
	player.call("set_facing", player.get("yaw") if false else 0.0, -0.35)
	await _steps(10)
	var drawn := await _shot("first_drawn")
	print("R4_IRON first drawn hands=", player_arms.call("play_path_observation").get("hands", []))
	await _view(false)
	await _steps(60)
	await _shot("third_drawn")
	print("R4_IRON third drawn hands=", player_body.call("play_path_observation").get("hands", []))
	player.call("toggle_weapon_ready")
	await _steps(90)
	await _shot("third_sheathed")
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 iron kit")
	quit(0 if failures == 0 else 1)
