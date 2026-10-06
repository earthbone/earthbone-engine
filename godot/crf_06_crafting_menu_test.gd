extends SceneTree

## CRF-06 runtime check for placed bench activation and recipe button wiring.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _recipe_buttons(player: Node3D) -> Array[Button]:
	var result: Array[Button] = []
	var content := player.get_node("MenuLayer/MenuPanel/Layout/Content") as VBoxContainer
	if content.get_child_count() == 0:
		return result
	var scroll := content.get_child(0) as ScrollContainer
	if scroll.get_child_count() == 0:
		return result
	var rows := scroll.get_child(0) as VBoxContainer
	for child in rows.get_children():
		if child is Button:
			result.append(child as Button)
	return result

func _region_has_ink(image: Image, control: Control) -> bool:
	var rect := control.get_global_rect()
	var left := maxi(0, floori(rect.position.x))
	var top := maxi(0, floori(rect.position.y))
	var right := mini(image.get_width(), ceili(rect.end.x))
	var bottom := mini(image.get_height(), ceili(rect.end.y))
	for y in range(top, bottom):
		for x in range(left, right):
			if image.get_pixel(x, y).get_luminance() > 0.2:
				return true
	return false

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: crf_06_crafting_menu_test requires SKYRIM_DATA and the extension")
		quit(1)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "player did not share game records")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"player collision did not load")
	_check(player.call("start_gameplay"), "game runtime did not start")

	var benches: Array = player.call("gameplay_creation_benches")
	_check(not benches.is_empty(), "Bannered Mare has no loaded creation bench")
	var opened_with_rows := false
	for bench: Dictionary in benches:
		var activated: bool = player.call("activate_reference", bench.file, bench.object)
		_check(activated, "placed creation bench activation failed")
		_check(player.call("is_menu_open"), "bench activation did not open the native menu")
		if not player.call("is_menu_open"):
			continue
		await process_frame
		await process_frame
		var buttons := _recipe_buttons(player)
		if not buttons.is_empty():
			opened_with_rows = true
			var panel := player.get_node("MenuLayer/MenuPanel") as Control
			_check(panel.is_visible_in_tree(), "recipe menu was not visible in the scene tree")
			var rendered := get_root().get_texture().get_image()
			var enabled: Button
			for button in buttons:
				if not button.disabled:
					enabled = button
					break
			var recipe_button := enabled if enabled != null else buttons[0]
			var wired := false
			for connection: Dictionary in recipe_button.get_signal_connection_list("pressed"):
				var callback: Callable = connection.callable
				if callback.get_object() == player and callback.get_method() == "confirm_crafting_recipe":
					wired = true
			_check(wired, "recipe button is not connected to PlayerNode confirmation")
			_check(recipe_button.is_visible_in_tree(), "recipe button was not visible in the scene tree")
			_check(_region_has_ink(rendered, recipe_button), "recipe button produced no viewport pixels")
			if enabled == null:
				# A fresh runtime player has no inventory. Exercise the connected
				# callback's rejected Game::craft path without changing any state.
				recipe_button.disabled = false
			var click := InputEventMouseButton.new()
			click.button_index = MOUSE_BUTTON_LEFT
			click.position = recipe_button.get_global_rect().get_center()
			click.pressed = true
			Input.parse_input_event(click)
			Input.flush_buffered_events()
			await process_frame
			click = InputEventMouseButton.new()
			click.button_index = MOUSE_BUTTON_LEFT
			click.position = recipe_button.get_global_rect().get_center()
			click.pressed = false
			Input.parse_input_event(click)
			Input.flush_buffered_events()
			_check(player.call("is_menu_open"), "recipe confirmation closed the bench menu")
			_check(not _recipe_buttons(player).is_empty(),
				"recipe widget disappeared after confirmation")
			break
		player.call("close_menu")
		await process_frame
	_check(opened_with_rows, "no creation bench presented a recipe button")

	player.call("close_menu")
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " crf_06 crafting menu")
	quit(0 if failures == 0 else 1)
