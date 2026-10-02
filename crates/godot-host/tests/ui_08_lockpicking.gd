extends SceneTree

## A live Godot interaction spends a pick on a miss, then opens the same door.
var failures := 0
var player: Node3D
var streamer: Node3D

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _find_named(node: Node, wanted: String) -> Node:
	if node.name == wanted:
		return node
	for child in node.get_children():
		var found := _find_named(child, wanted)
		if found != null:
			return found
	return null

func _button(node: Node, wanted: String) -> Button:
	if node is Button and node.text == wanted:
		return node
	for child in node.get_children():
		var found := _button(child, wanted)
		if found != null:
			return found
	return null

func _run() -> void:
	var data := OS.get_environment("SOE_UI_TEST_DATA")
	if data.is_empty():
		print("SKIP: set SOE_UI_TEST_DATA to the UI-08 fixture output")
		quit(0)
		return
	player = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	streamer = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "fixture data did not load")
	_check(player.call("use_streamer_data", streamer), "PlayerNode did not share fixture data")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"fixture cell collision did not load")
	_check(player.call("start_gameplay"), "shared Game did not start")
	_check(not player.call("activate_reference", 0, 0x511),
		"locked door activated before picking")
	_check(player.call("is_menu_open"), "locked door did not open lockpicking")
	var content: Control = player.call("menu_content")
	var angle: HSlider = _find_named(content, "PickAngle")
	var turn: Button = _find_named(content, "TurnLock")
	var status: Label = _find_named(content, "LockpickStatus")
	_check(angle != null, "lockpick angle control was not presented")
	_check(turn != null, "lockpick turn control was not presented")
	_check(status != null, "lockpick status was not presented")
	if angle != null and turn != null and status != null:
		angle.value = 0.0
		turn.pressed.emit()
		_check(player.call("is_menu_open"), "failed turn closed the interaction")
		_check(status.text == "The pick broke. Adjust the angle and try again.",
			"failed turn did not report the broken pick")
		player.call("close_menu")
		_check(player.call("open_inventory_menu"), "inventory did not open after failed turn")
		_check(_button(player, "Lockpick x1") != null,
			"failed turn did not consume exactly one lockpick")
		player.call("close_menu")
		_check(not player.call("activate_reference", 0, 0x511),
			"locked door did not reopen lockpicking after the miss")
		content = player.call("menu_content")
		angle = _find_named(content, "PickAngle")
		turn = _find_named(content, "TurnLock")
		_check(angle != null and turn != null, "second lockpick controls were not presented")
		if angle != null and turn != null:
			angle.value = 23.6
			turn.pressed.emit()
			_check(not player.call("is_menu_open"), "successful turn did not close interaction")
			_check(player.call("activate_reference", 0, 0x511),
				"successful turn did not unlock the shared Game door")
			_check(not player.call("is_menu_open"), "unlocked activation reopened lockpicking")
	_check(not player.call("activate_reference", 0, 0x513),
		"key-only lock activated without its key")
	_check(not player.call("is_menu_open"), "key-only lock entered the lockpicking path")
	await process_frame
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " UI-08 live lockpicking")
	quit(0 if failures == 0 else 1)
