extends SceneTree

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
		printerr("FAIL: scr_30_training_render_test requires SKYRIM_DATA and the extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "player rejected shared game data")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"player could not load the Bannered Mare")
	_check(player.call("start_gameplay"), "game runtime did not start")
	_check(player.call("debug_invoke_game_actor_native", "ShowTrainingMenu", "HuldaRef"),
		"Game.ShowTrainingMenu Papyrus call failed for the loaded actor")
	player.call("advance", 1.0 / 60.0)
	await process_frame
	var panel := player.get_node_or_null("MenuLayer/MenuPanel") as Control
	_check(panel != null and panel.visible, "training menu was not rendered")
	var title := player.get_node_or_null("MenuLayer/MenuPanel/Layout/Title") as Label
	_check(title != null and title.text == "Training", "rendered menu title is not Training")
	var content: Control = player.call("menu_content")
	_check(content != null and content.get_child_count() == 1,
		"training presenter did not add its content container")
	if content != null and content.get_child_count() == 1:
		var scroll := content.get_child(0) as ScrollContainer
		_check(scroll != null and scroll.get_child_count() == 1,
			"training content does not contain its scrollable rows")
		if scroll != null and scroll.get_child_count() == 1:
			var rows := scroll.get_child(0) as VBoxContainer
			_check(rows != null and rows.get_child_count() > 0,
				"training presenter rendered no offer or unavailable state")
			if rows != null and rows.get_child_count() > 0:
				var first := rows.get_child(0)
				_check(first is Label and not (first as Label).text.is_empty(),
					"training presenter did not render its heading or unavailable message")
				if rows.get_child_count() > 1:
					var purchase := rows.get_child(1) as Button
					_check(purchase != null and purchase.text.begins_with("Train for "),
						"available training offer did not render its purchase button")
	player.call("close_menu")
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " SCR-30 training presentation")
	quit(0 if failures == 0 else 1)
