extends SceneTree

## Exercise the native MCM buttons with unmodified SkyUI lifecycle scripts.
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

func _run() -> void:
	var directory := OS.get_environment("SOE_UI_05_FIXTURE_DIR")
	if directory.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: UI-05 requires the fixture and built extension")
		quit(1)
		return
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("debug_load_mcm_fixture", directory), "fixture could not load")
	_check(player.call("open_mcm_menu"), "MCM list did not open")
	await process_frame
	await process_frame
	var configs := _buttons(player, "Example Mod")
	_check(configs.size() == 1, "registered config was not rendered once")
	if configs.size() == 1:
		configs[0].emit_signal("pressed")
	await process_frame
	await process_frame
	var general := _buttons(player, "General")
	var advanced := _buttons(player, "Advanced")
	_check(general.size() == 1 and advanced.size() == 1, "declared page list differs")
	if general.size() == 1 and advanced.size() == 1:
		_check(general[0].get_global_rect().size.y > 0, "General button has no layout")
		_check(advanced[0].get_global_rect().position.y > general[0].get_global_rect().position.y,
			"page order differs from the script")
	var state: Dictionary = player.call("debug_mcm_fixture_state")
	_check(state.get("init_count") == 1 and state.get("register_count") == 1, "init/registration callbacks differ")
	_check(state.get("sequence") == 12 and state.get("last_page") == "", "empty reset did not precede open")
	await RenderingServer.frame_post_draw
	var image := get_root().get_texture().get_image()
	_check(image.get_width() == 640 and image.get_height() == 360, "window was not drawn")
	_check(image.save_png(directory.path_join("mcm-pages.png")) == OK, "capture could not be saved")
	if advanced.size() == 1:
		advanced[0].emit_signal("pressed")
	await process_frame
	state = player.call("debug_mcm_fixture_state")
	_check(state.get("last_page") == "Advanced" and state.get("reset_count") == 2, "button did not reach OnPageReset")
	_check(not player.call("mcm_select_page", 2), "undeclared page was accepted")
	player.call("close_menu")
	state = player.call("debug_mcm_fixture_state")
	_check(state.get("close_count") == 1, "close callback differs")
	_check(state.get("errors", PackedStringArray()).is_empty(), "VM reported lifecycle errors")
	player.free()
	await process_frame
	print("PASS ui_05 windowed MCM lifecycle" if failures == 0 else "FAIL ui_05 windowed MCM lifecycle")
	quit(0 if failures == 0 else 1)
