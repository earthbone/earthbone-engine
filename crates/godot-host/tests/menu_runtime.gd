extends SceneTree

## Native menu focus, input and shared-game pause through the scene-tree player.
var failures := 0
var opened := 0
var closed := 0
var player: Node3D

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _key(code: Key, pressed: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = code
	event.pressed = pressed
	Input.parse_input_event(event)
	Input.flush_buffered_events()

func _count_nodes(node: Node) -> int:
	var count := 1
	for child in node.get_children():
		count += _count_nodes(child)
	return count

func _run() -> void:
	player = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	_check(not player.call("open_menu", "Before ready"), "unready menu opened")
	get_root().add_child(player)
	var previous := Button.new()
	previous.text = "Previous focus"
	get_root().add_child(previous)
	previous.grab_focus()
	player.connect("menu_opened", func(_title: String):
		opened += 1
		_check(player.call("is_menu_open"), "opened callback cannot inspect player"))
	player.connect("menu_closed", func(): closed += 1)
	var panel: Control = player.get_node("MenuLayer/MenuPanel")
	var close: Button = panel.get_node("Layout/Close")
	close.focus_entered.connect(func():
		_check(player.call("is_menu_open"), "menu focus callback cannot inspect player"))
	previous.focus_entered.connect(func():
		_check(not player.call("is_menu_open"), "restored focus callback cannot inspect player"))
	_check(panel.theme != null, "native Theme is missing")
	_check(not panel.visible, "menu starts visible")
	var baseline := _count_nodes(player)
	_check(_count_nodes(player.get_node("MenuLayer")) == 6, "framework node count changed")
	_check(not player.call("open_menu", "  "), "empty menu title opened")
	var old_mouse := Input.mouse_mode
	_check(player.call("open_menu", "Native menu"), "menu did not open")
	_check(not player.call("open_menu", "Second menu"), "nested menu replaced state")
	_check(panel.visible and close.has_focus(), "menu did not show and focus Close")
	_check(Input.mouse_mode == Input.MOUSE_MODE_VISIBLE, "menu retained mouse capture")
	var content: Control = player.call("menu_content")
	var label := Label.new()
	label.text = "Presenter content"
	content.add_child(label)
	var facing: Vector2 = player.call("facing")
	var motion := InputEventMouseMotion.new()
	motion.relative = Vector2(30, 20)
	Input.parse_input_event(motion)
	_key(KEY_F, true)
	Input.action_press("Toggle POV")
	player.call("advance", 1.0)
	_check(player.call("facing") == facing, "menu let mouse look through")
	_check(not player.call("is_third_person"), "menu let gameplay input through")
	await process_frame
	_check(opened == 1, "open notification was not delivered once")
	close.pressed.emit()
	_check(not player.call("is_menu_open") and not panel.visible, "Close button did not close")
	_check(previous.has_focus(), "previous Control focus was not restored")
	_check(Input.mouse_mode == old_mouse, "previous mouse mode was not restored")
	_check(player.call("menu_content") == null, "closed menu exposes content")
	_check(not player.call("close_menu"), "repeat close changed state")
	player.call("advance", 1.0)
	_check(not player.call("is_third_person"), "close replayed a queued gameplay press")
	_key(KEY_F, false)
	await process_frame
	await process_frame
	_check(not is_instance_valid(label), "closed menu leaked content")
	_check(closed == 1, "close notification was not delivered once")
	_key(KEY_ESCAPE, true)
	_check(player.call("is_menu_open"), "Escape did not open the native menu")
	_key(KEY_ESCAPE, false)
	_key(KEY_ESCAPE, true)
	_check(not player.call("is_menu_open"), "Escape did not close the native menu")
	_key(KEY_ESCAPE, false)
	await process_frame
	await process_frame
	_check(_count_nodes(player) == baseline, "repeated menu lifetime grew the tree")
	previous.grab_focus()
	player.call("open_menu", "Focus loss")
	previous.free()
	get_root().focus_exited.emit()
	player.call("close_menu")
	_check(Input.mouse_mode == Input.MOUSE_MODE_VISIBLE, "close captured the unfocused window")
	get_root().focus_entered.emit()
	_check(Input.mouse_mode == old_mouse, "focus regain lost saved mouse mode")
	await process_frame
	await process_frame
	Input.action_press("Toggle POV")
	player.call("advance", 1.0 / 60.0)
	_check(player.call("is_third_person"), "gameplay input did not resume")
	Input.action_release("Toggle POV")

	var data := OS.get_environment("SOE_UI_TEST_DATA")
	if data.is_empty():
		data = OS.get_environment("SKYRIM_DATA")
	if not data.is_empty():
		var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
		get_root().add_child(streamer)
		_check(streamer.call("load_data", data), "test data did not load")
		_check(player.call("use_streamer_data", streamer), "player did not share game records")
		_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
			"test cell did not load")
		_check(player.call("start_gameplay"), "shared Game did not start")
		var days: float = player.call("gameplay_days")
		var position: Vector3 = player.call("position")
		player.call("open_menu", "Pause shared Game")
		for index in range(10):
			player.call("advance", 1.0)
		_check(player.call("gameplay_days") == days, "menu advanced the shared game clock")
		_check(player.call("position") == position, "menu advanced host physics")
		player.call("close_menu")
		player.call("advance", 1.0 / 60.0)
		_check(player.call("gameplay_days") > days, "close did not resume the shared game clock")
		streamer.free()
	else:
		print("SKIP: shared Game, SOE_UI_TEST_DATA and SKYRIM_DATA unset")
	player.call("open_menu", "Shutdown")
	player.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " native menu runtime")
	quit(0 if failures == 0 else 1)
