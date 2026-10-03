extends SceneTree

## UI-18 runs the Game main-menu natives through PlayerNode in Godot.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: ui_18_main_menu_test requires SKYRIM_DATA and the extension")
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

	var returned: Array[bool] = []
	var titles: Array[String] = []
	player.returned_to_main_menu.connect(func(): returned.append(true))
	player.title_sequence_started.connect(func(name: String): titles.append(name))
	_check(player.call("debug_invoke_main_menu_native", "StartTitleSequence", "MQ101Intro"),
		"StartTitleSequence was not invoked")
	_check(player.call("is_main_menu"), "title sequence did not enter the main menu")
	_check(player.call("active_title_sequence") == "MQ101Intro",
		"host did not retain the requested title sequence")
	_check(titles == ["MQ101Intro"], "host did not emit the named title sequence")
	_check(returned.size() == 1, "title sequence did not return from gameplay")
	_check(player.call("gameplay_cell").is_empty(), "title sequence retained the active cell")

	_check(player.call("use_streamer_data", streamer), "player could not reattach game records")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"player collision did not reload")
	_check(player.call("start_gameplay"), "game runtime did not restart")
	_check(not player.call("is_main_menu"), "new gameplay did not leave the main menu")
	_check(player.call("debug_invoke_main_menu_native", "QuitToMainMenu", ""),
		"QuitToMainMenu was not invoked")
	_check(player.call("is_main_menu"), "quit did not return to the main menu")
	_check(player.call("active_title_sequence").is_empty(), "quit retained the prior title sequence")
	_check(player.call("gameplay_cell").is_empty(), "quit retained the active cell")
	_check(returned.size() == 2, "quit did not emit the return signal")
	_check(not player.call("is_menu_open"), "quit left a modal menu open")

	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " UI-18 main menu lifecycle")
	quit(0 if failures == 0 else 1)
