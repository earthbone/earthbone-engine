# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree
## UI-27 checks the live container presenter and quits after capturing it.

var failures: Array[String] = []

func _initialize() -> void:
	call_deferred("run")

func check(condition: bool, message: String) -> void:
	if not condition:
		failures.append(message)
		push_error("UI-27: " + message)

func frames() -> void:
	for index in range(5):
		await process_frame
	await RenderingServer.frame_post_draw

func panel_check(player: Node3D) -> Rect2:
	var panel := player.get_node("MenuLayer/MenuPanel") as Control
	var viewport := root.get_visible_rect()
	var expected := Rect2(viewport.size * Vector2(0.04, 0.08), viewport.size * Vector2(0.92, 0.84))
	var actual := panel.get_global_rect()
	check(actual.position.distance_to(expected.position) < 1.0, "panel position differs from SkyUI layout")
	check(actual.size.distance_to(expected.size) < 1.0, "panel size differs from SkyUI layout: " + str(actual))
	check(viewport.encloses(actual), "panel exceeds viewport")
	var content: Control = player.call("menu_content")
	var menu = content.get_node("ItemMenu")
	check(menu.mode == "container", "container presenter did not open")
	check(menu.tabs.get_child(0).text == "Take" and menu.tabs.get_child(1).text == "Store", "container tabs missing")
	check(menu.categories.get_global_rect().end.x <= menu.header.get_global_rect().position.x, "categories must precede columns")
	check(menu.header.get_global_rect().end.x <= menu.card.get_global_rect().position.x, "card must follow columns")
	check(actual.encloses(menu.card.get_global_rect()), "card exceeds panel")
	check(menu.search.visible and menu.header.visible, "search or sortable columns hidden")
	if OS.get_environment("SOE_UI_27_BUNDLED") == "1":
		check(menu.cache_directory.is_empty(), "failed conversion retained a cache")
		for decoration in ["Background", "Frame"]:
			var art = menu.get_node(decoration)
			check(art.art_source.begins_with("res://ui/theme/") and not art.meshes.is_empty(), "missing bundled " + decoration)
		for category in menu.categories.get_children():
			var icon = category.get_node("CategoryIcon")
			check(icon.art_source.begins_with("res://ui/theme/") and not icon.meshes.is_empty(), "missing bundled category " + category.name)
	return actual

func run() -> void:
	ProjectSettings.set_setting("soe/tools/path", OS.get_environment("SOE_UI_27_TOOLS"))
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	if not streamer.call("load_data", OS.get_environment("SKYRIM_DATA")):
		quit(1)
		return
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	if not player.call("use_streamer_data", streamer) or player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) <= 0 or not player.call("start_gameplay"):
		quit(1)
		return
	var holder: Dictionary = {}
	for candidate in player.call("gameplay_containers"):
		if player.call("open_container_inventory", candidate.file, candidate.object):
			holder = candidate
			break
	if holder.is_empty():
		push_error("UI-27: no vanilla container opened")
		quit(1)
		return
	await frames()
	var first := panel_check(player)
	var capture := OS.get_environment("SOE_UI_27_CAPTURE")
	check(root.get_texture().get_image().save_png(capture) == OK, "capture failed")
	check(player.call("close_menu"), "close failed")
	check(player.call("open_container_inventory", holder.file, holder.object), "reopen failed")
	await frames()
	check(panel_check(player) == first, "reopening changed panel bounds")
	check(player.call("close_menu"), "second close failed")
	player.free()
	streamer.free()
	await process_frame
	print("UI-27 PANEL ", first)
	if failures.is_empty():
		print("UI-27 PASS")
	quit(0 if failures.is_empty() else 1)
