extends SceneTree

var failures := 0

func _initialize() -> void:
	call_deferred("ui_04_renders_installed_quest_journal_rows")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _labels(node: Node) -> Array[String]:
	var result: Array[String] = []
	if node is Label:
		result.append((node as Label).text)
	for child in node.get_children():
		result.append_array(_labels(child))
	return result

func ui_04_renders_installed_quest_journal_rows() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("SKIP: SKYRIM_DATA is unset")
		quit(0)
		return

	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "Skyrim data did not load")
	_check(player.call("use_streamer_data", streamer), "player did not share streamer data")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"Bannered Mare cell did not load")
	_check(player.call("start_gameplay"), "shared Game did not start")
	_check(player.call("open_quest_journal"), "quest journal did not open")
	var panel: Control = player.get_node("MenuLayer/MenuPanel")
	_check(panel.visible, "quest journal panel is hidden")
	_check((panel.get_node("Layout/Title") as Label).text == "Quests",
		"quest journal title was not rendered")
	await process_frame
	var content: Control = player.call("menu_content")
	var labels := _labels(content)
	_check(not labels.is_empty(), "journal rendered no quest or objective rows")
	_check(not labels.has("No active quests"), "installed Game started with no active journal rows")
	_check(labels.size() > 1, "journal rendered no objective text")
	player.call("close_menu")
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ui_04 quest journal host rendering")
	quit(0 if failures == 0 else 1)
