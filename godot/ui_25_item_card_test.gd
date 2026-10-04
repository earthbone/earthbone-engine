extends SceneTree

## UI-25 installed-data window check for names, model pixels and menu selection.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _model_pixels(viewport: SubViewport) -> int:
	var image := viewport.get_texture().get_image()
	var pixels := 0
	for y in range(image.get_height()):
		for x in range(image.get_width()):
			var color := image.get_pixel(x, y)
			if color.a > 0.1 and color.get_luminance() > 0.02:
				pixels += 1
	return pixels

func _frames() -> void:
	for index in range(5):
		await process_frame
	await RenderingServer.frame_post_draw

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: ui_25 requires SKYRIM_DATA and the release extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "installed data did not load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data was not attached")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"Bannered Mare collision was not loaded")
	_check(player.call("start_gameplay"), "localized gameplay startup failed")
	_check(player.call("open_inventory_menu"), "inventory menu did not open")
	var content: Control = player.call("menu_content")
	var card := content.get_node("ItemMenu/ItemCard") as VBoxContainer
	var viewport := card.get_node("ModelPreview/Viewport") as SubViewport
	var pivot := viewport.get_node("ItemModel") as Node3D
	_check(viewport.own_world_3d, "item model shares the gameplay world")
	var started := Time.get_ticks_usec()
	_check(player.call("preview_inventory_item", 0, 0x12eb7), "Iron Sword did not build")
	print("UI-25 sword build_us=", Time.get_ticks_usec() - started, " surface_nodes=", pivot.get_child_count())
	await _frames()
	_check(card.get_node("ItemName").text == "Iron Sword", "FULL name did not resolve at startup")
	var stats: String = card.get_node("Statistics").text
	_check(stats.contains("Weight  9") and stats.contains("Value  25") and stats.contains("Damage  7"),
		"weapon card did not show record weight, value and damage: " + stats)
	_check(pivot.get_child_count() > 0, "NIF model produced no preview mesh")
	_check(_model_pixels(viewport) > 100, "sword preview produced fewer than 100 visible pixels")
	var initial_rotation := pivot.rotation.y
	var initial_image := viewport.get_texture().get_image().get_data()
	await create_timer(0.4).timeout
	await _frames()
	_check(absf(pivot.rotation.y - initial_rotation) > 0.05, "model did not rotate while menu mode was active")
	_check(initial_image != viewport.get_texture().get_image().get_data(), "rotation did not change rendered pixels")
	get_root().get_texture().get_image().save_png("res://../target/ui_25_inventory.png")
	viewport.get_texture().get_image().save_png("res://../target/ui_25_sword.png")
	_check(player.call("preview_inventory_item", 0, 0x12e49), "Iron Armor ground model did not build")
	await _frames()
	_check(card.get_node("ItemName").text == "Iron Armor", "armor FULL name did not resolve")
	_check(card.get_node("Statistics").text.contains("Armor"), "armor rating was absent")
	_check(_model_pixels(viewport) > 100, "armor ground model produced fewer than 100 visible pixels")
	_check(not player.call("preview_inventory_item", -1, 0), "invalid form identity was accepted")
	_check(player.call("close_menu"), "inventory did not close")
	await _frames()
	_check(not is_instance_valid(viewport), "closed menu retained its SubViewport")

	var opened_container := false
	var selected_name := ""
	for holder in player.call("gameplay_containers"):
		if not player.call("open_container_inventory", holder.file, holder.object):
			continue
		content = player.call("menu_content")
		var rows := content.get_node("ItemMenu/ItemList").get_child(0)
		var item_buttons: Array[Button] = []
		for child in rows.get_children():
			if child is Button and child.text.begins_with("Take "):
				item_buttons.append(child)
		if item_buttons.is_empty():
			player.call("close_menu")
			continue
		for button in item_buttons:
			_check(not button.text.contains("<string ") and not button.text.contains("Unnamed item"),
				"container contains an unresolved name: " + button.text)
		var selected: Button = item_buttons.back()
		selected.grab_focus()
		await _frames()
		card = content.get_node("ItemMenu/ItemCard")
		selected_name = card.get_node("ItemName").text
		_check(selected.text.begins_with("Take " + selected_name + " x"), "focus did not select the matching card")
		selected.emit_signal("mouse_entered")
		_check(card.get_node("ItemName").text == selected_name, "mouse selection did not match keyboard selection")
		selected.emit_signal("pressed")
		await _frames()
		get_root().get_texture().get_image().save_png("res://../target/ui_25_container.png")
		player.call("close_menu")
		opened_container = true
		break
	_check(opened_container, "no vanilla container listed its FULL item names")
	_check(player.call("open_inventory_menu"), "player inventory did not reopen after taking an item")
	content = player.call("menu_content")
	var found := false
	for button in content.get_node("ItemMenu/ItemList").get_child(0).get_children():
		if button is Button and button.text.begins_with(selected_name + " x"):
			found = true
	_check(found, "taken container item did not appear by FULL name in player inventory")
	_check(player.call("inventory_category", 1), "category refresh did not rebuild the item card")
	_check(content.get_node("ItemMenu/ItemCard") != null, "category refresh lost the card")
	player.call("close_menu")
	await _frames()
	_check(content.get_child_count() == 0, "closing retained presenter controls")
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ui_25_item_card_render_and_selection")
	quit(0 if failures == 0 else 1)
