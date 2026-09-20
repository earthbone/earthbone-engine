extends SceneTree

## Native item clicks read and change the same Game used by PlayerNode.
var failures := 0
var player: Node3D

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _button(node: Node, text: String) -> Button:
	if node is Button and node.text == text:
		return node
	for child in node.get_children():
		var found := _button(child, text)
		if found != null:
			return found
	return null

func _click(text: String) -> void:
	var button := _button(player, text)
	_check(button != null, "missing button: " + text)
	if button != null:
		button.focus_exited.connect(func():
			_check(player.call("is_menu_open"), "focus callback lost the menu"))
		button.grab_focus()
		button.pressed.emit()

func _run() -> void:
	player = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(not player.call("open_inventory_menu"), "inventory opened without a Game")
	_check(not player.call("is_menu_open"), "missing Game left an empty modal")
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", OS.get_environment("SOE_UI_TEST_DATA")), "data load failed")
	_check(player.call("use_streamer_data", streamer), "shared data failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"cell collision did not load")
	_check(player.call("start_gameplay"), "shared Game did not start")
	var key := InputEventKey.new()
	key.keycode = KEY_I
	key.pressed = true
	Input.parse_input_event(key)
	Input.flush_buffered_events()
	_check(player.call("is_menu_open"), "I did not open inventory")
	_check(_button(player, "Fixture sword x2") != null, "authored inventory missing")
	_click("Food")
	_check(_button(player, "Fixture bread x3") != null, "food missing")
	_check(_button(player, "Fixture sword x2") == null, "food category contains weapon")
	_click("Weapons")
	_click("Fixture sword x2")
	_check(_button(player, "Fixture sword x2 [Equipped]") != null, "click did not equip in Game")
	_check(_button(player, "Fixture bread x3") == null, "equip reset the selected category")
	_click("Fixture sword x2 [Equipped]")
	_check(_button(player, "Fixture sword x2") != null, "click did not unequip in Game")
	_check(not player.call("toggle_inventory_item", 0, 0x999), "unknown item equipped")
	player.call("close_menu")
	_check(not player.call("toggle_inventory_item", 0, 0x500), "closed handler changed equipment")
	player.call("open_menu", "Different menu")
	_check(not player.call("inventory_category", 0), "inventory replaced another presenter")
	player.call("close_menu")
	await process_frame
	await process_frame
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " native inventory runtime")
	quit(0 if failures == 0 else 1)
