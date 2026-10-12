extends "res://walkthrough.gd"

## HST-24 exercises the walkthrough process loop and its normal shutdown paths.
var failures := 0
var mode := ""


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("HST-24 FAIL: ", message)


func _key(pressed: bool, echo := false) -> void:
	var event := InputEventKey.new()
	event.keycode = KEY_ESCAPE
	event.pressed = pressed
	event.echo = echo
	Input.parse_input_event(event)
	Input.flush_buffered_events()


func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)


func _start() -> void:
	mode = OS.get_environment("SOE_HST_24_MODE")
	create_timer(60.0).timeout.connect(func():
		printerr("HST-24 timed out")
		quit(1))
	if mode == "metrics":
		await _metrics_check()
		return
	if OS.get_environment("SOE_HST_24_INSTALLED") == "1":
		startup_cell = "WhiterunOrigin"
		startup_radius = 0
		await super._start()
		if not loaded:
			return
	else:
		_key(true)
		await process_frame
		await process_frame
		_key(false)
		print("hst_24_escape_during_startup: PASS")
		player = ClassDB.instantiate("PlayerNode")
		_configure_player()
		get_root().add_child(player)
		system_overlay = SystemOverlay.new()
		system_overlay.player = player
		get_root().add_child(system_overlay)
		loaded = true
	await _pause_check()


func _metrics_check() -> void:
	var overlay := SystemOverlay.new()
	_check(overlay.godot_metrics().is_empty(), "unattached overlay read a viewport")
	get_root().add_child(overlay)
	_check(overlay.godot_metrics().has("visible"), "attached overlay lost render metrics")
	get_root().remove_child(overlay)
	_check(overlay.godot_metrics().is_empty(), "detached overlay read a viewport")
	overlay.free()
	print("hst_24_overlay_metrics_without_viewport: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)


func _pause_check() -> void:
	await process_frame
	var before := frames
	var days: float = player.call("gameplay_days")
	var position: Vector3 = player.call("position")
	_key(true)
	_check(player.call("is_menu_open"), "first Escape did not open the pause menu")
	var panel: Control = player.get_node("MenuLayer/MenuPanel")
	_check(panel.get_node("Layout/Title").text == "Paused", "Escape opened the wrong menu")
	_check(panel.visible, "pause menu is hidden")
	_check(panel.get_node("Layout/Close").has_focus(), "pause menu lost keyboard focus")
	_check(Input.mouse_mode == Input.MOUSE_MODE_VISIBLE, "pause menu captured the mouse")
	_key(true, true)
	for _frame in range(4):
		await process_frame
		player.call("advance", 1.0 / 60.0)
	_check(frames > before, "walkthrough stopped processing while Escape was held")
	_check(player.call("is_menu_open"), "held Escape closed the menu")
	_check(player.call("gameplay_days") == days, "pause advanced the game clock")
	_check(player.call("position") == position, "pause moved the player")
	_key(false)
	_key(true)
	_check(not player.call("is_menu_open"), "second Escape did not close the menu")
	_check(not panel.visible, "closed pause menu stayed visible")
	_key(false)
	await process_frame
	await process_frame
	_check(panel.get_node("Layout/Content").get_child_count() == 0, "closed pause menu retained Quit")
	if OS.get_environment("SOE_HST_24_INSTALLED") == "1":
		player.call("advance", 1.0 / 60.0)
		_check(player.call("gameplay_days") > days, "closing pause did not resume the game clock")
	_key(true)
	_key(false)
	await process_frame
	var content: Control = player.call("menu_content")
	_check(content.get_child_count() == 1, "reopening duplicated the Quit entry")
	var quit_button: Button = content.get_node("Quit")
	_check(quit_button.text == "Quit", "pause menu has no Quit entry")
	if failures > 0:
		quit(1)
		return
	print("hst_24_escape_toggles_pause: PASS")
	if mode == "window":
		_check(auto_accept_quit, "window close requests are disabled")
		print("hst_24_window_close: PASS")
		get_root().close_requested.emit()
	else:
		print("hst_24_pause_quit_entry: PASS")
		quit_button.pressed.emit()
	await process_frame
	await process_frame
	printerr("HST-24 requested shutdown did not stop the tree")
	quit(1)
