# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree

const Menu = preload("res://ui/item_menu.gd")
var failures: Array[String] = []
var actions: Array = []

func _initialize() -> void:
	call_deferred("run")

func check(condition: bool, message: String) -> void:
	if not condition:
		failures.append(message)
		push_error("UI-20: " + message)

func item(id: String, title: String, kind: String, weight: Variant, value: int) -> Dictionary:
	return {"id": id, "name": title, "category": kind, "icon": "default_weapon" if kind == "Weapons" else "default_misc", "weight": weight, "value": value, "count": 8, "equipped": false, "favorite": true, "side": 0, "description": "An original test item"}

func ids(menu: Control) -> Array:
	return menu.visible_rows.map(func(row): return row.id)

func run() -> void:
	var menu := Menu.new()
	root.add_child(menu)
	menu.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	var data := [item("1", "Ébony blade", "Weapons", 5.0, 100), item("2", "apple", "Food", 1.0, 2), item("3", "Arrow", "Weapons", 0.0, 4), item("4", "Unknown", "Misc", null, 5)]
	menu.configure("inventory", data, OS.get_environment("SOE_INTERFACE_CACHE"))
	menu.activate_item.connect(func(row, amount, side): actions.append([row.id, amount, side]))
	await process_frame
	await process_frame
	print("UI-20 rows ", ids(menu), " category=", menu.category, " side=", menu.side)
	check(menu.column_order() == ["equipped", "icon", "name", "category", "weight", "value"], "default column order")
	check(ids(menu) == ["2", "3", "4", "1"], "case insensitive name order")
	menu.sort_by("weight")
	print("UI-20 weight descending ", ids(menu))
	check(ids(menu) == ["1", "2", "3", "4"], "numeric descending with missing last")
	menu.sort_by("weight")
	check(ids(menu) == ["3", "2", "1", "4"], "numeric ascending with missing last")
	menu.select_category("Weapons")
	check(menu.column_order() == ["equipped", "icon", "name", "category", "damage", "weight", "value"], "weapon column order")
	menu.search.text = "EBONY"
	menu.search.text_changed.emit("EBONY")
	await create_timer(0.35).timeout
	check(ids(menu) == ["1"], "accent normalized debounced search intersects category")
	check(menu.card.get_node("Name").text == "Ébony blade", "card follows filtered selection")
	menu.search.text = ""
	menu.search.text_changed.emit("")
	menu.select_category("All")
	menu.sort_by("name")
	await process_frame
	var category_rect := menu.categories.get_global_rect()
	var list_rect := menu.header.get_global_rect()
	var card_rect := menu.card.get_global_rect()
	check(category_rect.end.x <= list_rect.position.x, "category column left of item list")
	check(list_rect.end.x <= card_rect.position.x, "item card right of columns")
	check(root.get_visible_rect().encloses(card_rect), "card inside viewport")
	if not OS.get_environment("SOE_INTERFACE_CACHE").is_empty():
		var found := false
		for button in menu.rows.get_children():
			if button is Button:
				var icon = button.get_child(0).get_child(1)
				found = found or icon.art_source != "builtin"
		check(found, "installed SkyUI cache supplies item icons")
	await process_frame
	await RenderingServer.frame_post_draw
	var capture := OS.get_environment("SOE_UI_20_CAPTURE")
	if not capture.is_empty():
		check(root.get_texture().get_image().save_png(capture) == OK, "window capture")
	menu.configure("container", data)
	menu._activate(data[2])
	check(actions.back() == ["3", 8, 0], "weightless take transfers full stack")
	var stored: Dictionary = data[0].duplicate()
	stored.side = 1
	menu.replace_rows(data + [stored])
	menu.tabs.get_child(1).pressed.emit()
	print("UI-20 store ", ids(menu), " category=", menu.category, " query=", menu.query)
	check(ids(menu) == ["1"], "store tab reads the player's rows")
	menu._select(stored)
	menu.quantity.value = 3
	menu.primary.pressed.emit()
	check(actions.back() == ["1", 3, 1], "quantity confirmation retains source side")
	menu.configure("barter", data + [stored])
	check(menu.tabs.get_child(0).text == "Buy" and menu.tabs.get_child(1).text == "Sell", "barter tabs")
	menu.configure("favorites", data)
	menu.select_side(0)
	menu.select_category("Gear")
	check(ids(menu) == ["3", "1"], "favorites gear filter")
	menu.select_category("Aid")
	check(ids(menu) == ["2"], "favorites aid filter")
	menu.select_category("Magic")
	check(menu.visible_rows.is_empty() and menu.card.get_child_count() == 0, "empty filter clears card")
	menu.configure("magic", [item("5", "Flame", "Spells", null, 0)])
	menu.select_category("All")
	check(menu.column_order() == ["equipped", "icon", "name", "category", "cost"], "magic column order")
	menu.tabs.get_node("LeftHand").pressed.emit()
	check(menu.get_meta("hand") == 0, "left hand selection")
	menu.tabs.get_node("RightHand").pressed.emit()
	check(menu.get_meta("hand") == 1, "right hand selection")
	menu.queue_free()
	await process_frame
	if failures.is_empty():
		print("UI-20 PASS")
	quit(0 if failures.is_empty() else 1)
