extends SceneTree

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _check_menu(player: Node3D, title: String, mode: String) -> void:
	var panel := player.get_node_or_null("MenuLayer/MenuPanel") as Control
	_check(panel != null and panel.visible, title + " panel is not visible")
	var title_label := player.get_node_or_null("MenuLayer/MenuPanel/Layout/Title") as Label
	var title_matches := title_label != null and title_label.text == title
	if title == "Barter" and title_label != null:
		title_matches = title_label.text == "Barter" or title_label.text.begins_with("Barter: ")
	_check(title_matches, title + " title is wrong")
	var content: Control = player.call("menu_content")
	var presenter := content.get_node_or_null("ItemMenu") as Control if content != null else null
	_check(presenter != null, title + " item presenter is missing")
	if presenter != null:
		_check(presenter.get("mode") == mode, title + " presenter mode is wrong")

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: scr_13_actor_menu_host_test requires SKYRIM_DATA and the extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	_check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0, "Bannered Mare render failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"Bannered Mare collision failed")
	_check(player.call("start_gameplay"), "runtime start failed")
	await process_frame
	_check(player.call("debug_invoke_actor_menu_native", "ShowBarterMenu", "HuldaRef", false),
		"ShowBarterMenu Papyrus call failed")
	player.call("advance", 1.0 / 60.0)
	await process_frame
	_check_menu(player, "Barter", "barter")
	_check(player.call("close_menu"), "barter menu did not close")
	_check(player.call("debug_invoke_actor_menu_native", "OpenInventory", "HuldaRef", true),
		"forced OpenInventory Papyrus call failed")
	player.call("advance", 1.0 / 60.0)
	await process_frame
	var title_label := player.get_node_or_null("MenuLayer/MenuPanel/Layout/Title") as Label
	_check_menu(player, "Hulda", "container")
	_check(player.call("close_menu"), "actor inventory menu did not close")
	print("SCR-13 actor menu host presentation: ", "PASS" if failures == 0 else "FAIL")
	player.free()
	streamer.free()
	quit(0 if failures == 0 else 1)
