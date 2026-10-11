extends SceneTree

## Exercise script-authored text and toggle rows through production controls.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _buttons(node: Node, text: String) -> Array[Button]:
	var found: Array[Button] = []
	if node is Button and node.text == text and node.is_visible_in_tree():
		found.append(node)
	for child in node.get_children():
		found.append_array(_buttons(child, text))
	return found

func _option(node: Node, text: String) -> BaseButton:
	if node is HBoxContainer:
		for child in node.get_children():
			if child is Label and child.text == text:
				for control in node.get_children():
					if control is BaseButton:
						return control
	for child in node.get_children():
		var found := _option(child, text)
		if found != null:
			return found
	return null

func _run() -> void:
	var directory := OS.get_environment("SOE_UI_11_FIXTURE_DIR")
	if directory.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: UI-11 requires its fixture and built extension")
		quit(1)
		return
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("debug_load_mcm_fixture", directory), "fixture could not load")
	_check(player.call("open_mcm_menu"), "MCM did not open")
	await process_frame
	var configs := _buttons(player, "UI-11 Mod")
	_check(configs.size() == 1, "config was not rendered once")
	if configs.size() == 1:
		configs[0].emit_signal("pressed")
	await process_frame
	var pages := _buttons(player, "General")
	_check(pages.size() == 1, "page was not rendered once")
	if pages.size() == 1:
		pages[0].emit_signal("pressed")
	await process_frame
	await process_frame
	var text := _option(player, "Status")
	var toggle := _option(player, "Enabled")
	_check(text != null and toggle != null, "script options were not rendered")
	_check(_option(player, "Hidden") == null, "hidden option was rendered")
	if text != null and toggle != null:
		_check(text is Button and text.text == "Ready" and not text.disabled, "text value or flags differ")
		_check(toggle is CheckButton and toggle.button_pressed and toggle.disabled, "toggle value or flags differ")
		_check(text.get_global_rect().size.y > 0, "text row has no layout")
		_check(toggle.get_global_rect().position.y > text.get_global_rect().position.y, "authored row order differs")
		_check(not player.call("mcm_select_option", 0x101), "disabled toggle selected")
		_check(not player.call("mcm_select_option", 0x104), "hidden text selected")
		_check(not player.call("mcm_select_option", 0), "stale page selected")
		text.emit_signal("pressed")
	await process_frame
	await process_frame
	var state: Dictionary = player.call("debug_mcm_fixture_state")
	_check(state.get("selection_count") == 1, "one press did not deliver exactly one callback")
	_check(state.get("selected_option") == 0x100, "callback lost page ID")
	text = _option(player, "Status")
	toggle = _option(player, "Enabled")
	if text != null and toggle != null:
		_check(text is Button and text.text == "Selected" and text.disabled, "script text update did not reach the control")
		_check(toggle is CheckButton and not toggle.button_pressed and not toggle.disabled, "script toggle update did not reach the control")
	_check(not player.call("mcm_select_option", 0x100), "disabled text selected again")
	state = player.call("debug_mcm_fixture_state")
	_check(state.get("selection_count") == 1, "rejected selection delivered a callback")
	if toggle != null:
		toggle.set_pressed_no_signal(true)
		toggle.emit_signal("pressed")
	await process_frame
	await process_frame
	state = player.call("debug_mcm_fixture_state")
	_check(state.get("selection_count") == 2, "toggle press did not deliver exactly one callback")
	_check(state.get("selected_option") == 0x101, "toggle callback lost page ID")
	toggle = _option(player, "Enabled")
	_check(toggle != null and not toggle.button_pressed, "script toggle value was replaced by the control")
	_check(state.get("errors", PackedStringArray()).is_empty(), "VM reported option errors")
	await RenderingServer.frame_post_draw
	var image := get_root().get_texture().get_image()
	_check(image.get_width() == 640 and image.get_height() == 360, "window was not drawn at 640 by 360")
	_check(image.save_png(directory.path_join("mcm-options.png")) == OK, "capture could not be saved")
	player.call("close_menu")
	_check(not player.call("mcm_select_option", 0x100), "closed menu selected an option")
	player.free()
	await process_frame
	print("PASS ui_11 windowed MCM options" if failures == 0 else "FAIL ui_11 windowed MCM options")
	quit(0 if failures == 0 else 1)
