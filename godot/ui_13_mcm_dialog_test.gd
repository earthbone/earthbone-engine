extends SceneTree

## Verify native dialogs through the real installed MCM framework.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	get_root().gui_embed_subwindows = true
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	var directory := OS.get_environment("SOE_UI_13_FIXTURE_DIR")
	_check(player.call("debug_load_mcm_fixture", directory), "fixture load failed")
	_check(player.call("open_mcm_menu"), "config list did not open")
	_check(player.call("mcm_select_config", 0, 0x800), "config did not open")
	await process_frame
	_check(player.call("mcm_select_option", 0), "color dialog did not open")
	await process_frame
	var dialog := player.get_node_or_null("McmDialog") as ConfirmationDialog
	_check(dialog != null and dialog.visible, "native color dialog is invisible")
	if dialog != null:
		_check(dialog.size.x <= 640 and dialog.size.y <= 360, "color dialog exceeds the viewport")
		var picker: ColorPicker = dialog.get("picker")
		_check(picker.color.to_html(false) == "123456", "Papyrus start color was ignored")
		_check(dialog.get("default_color") == 0xabcdef, "Papyrus default color was ignored")
		var reset := dialog.find_child("Default", true, false) as Button
		_check(reset != null, "Default button is missing")
		if reset != null:
			reset.emit_signal("pressed")
			_check(picker.color.to_html(false) == "abcdef", "Default changed the wrong color")
		await RenderingServer.frame_post_draw
		_check(get_root().get_texture().get_image().save_png(directory.path_join("mcm-dialog.png")) == OK, "capture failed")
		dialog.emit_signal("canceled")
	await process_frame
	var state: Dictionary = player.call("debug_mcm_fixture_state")
	_check(state.get("color_accept_count") == 0, "cancel delivered an acceptance callback")
	_check(not player.call("mcm_dialog_accept", 0, 0), "canceled dialog accepted late input")
	_check(player.call("mcm_select_option", 0), "color dialog did not reopen")
	await process_frame
	dialog = player.get_node_or_null("McmDialog") as ConfirmationDialog
	if dialog != null:
		var picker: ColorPicker = dialog.get("picker")
		picker.color = Color8(0x65, 0x43, 0x21)
		dialog.emit_signal("confirmed")
	await process_frame
	state = player.call("debug_mcm_fixture_state")
	_check(state.get("color_accept_count") == 1, "color accept callback count differs")
	_check(not player.call("mcm_dialog_accept", 0, 0), "color callback repeated")
	_check(player.call("mcm_select_option", 1), "key dialog did not open")
	await process_frame
	var cancel_key := InputEventKey.new()
	cancel_key.keycode = KEY_ESCAPE
	cancel_key.pressed = true
	Input.parse_input_event(cancel_key)
	await process_frame
	_check(player.get_node_or_null("McmDialog") == null, "Escape did not close just the dialog")
	_check(player.call("mcm_select_option", 1), "Escape closed the whole MCM")
	await process_frame
	var key := InputEventKey.new()
	key.keycode = KEY_SPACE
	key.physical_keycode = KEY_SPACE
	key.pressed = true
	Input.parse_input_event(key)
	await process_frame
	state = player.call("debug_mcm_fixture_state")
	_check(state.get("key_count") == 1 and state.get("last_key") == 57, "key capture callback differs")
	_check(state.get("conflict_control") == "Jump" and state.get("conflict_name") == "", "vanilla conflict fields differ")
	_check(state.get("errors", PackedStringArray()).is_empty(), "VM reported dialog errors")
	_check(player.call("mcm_select_option", 0), "final color dialog did not open")
	player.call("close_menu")
	_check(player.get_node_or_null("McmDialog") == null, "closing MCM leaked its dialog")
	_check(not player.call("mcm_dialog_accept", 0, 0), "closed config accepted late input")
	player.free()
	await process_frame
	print("PASS ui_13 native dialogs" if failures == 0 else "FAIL ui_13 native dialogs")
	quit(0 if failures == 0 else 1)
