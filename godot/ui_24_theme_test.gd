# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree

const Menu = preload("res://ui/item_menu.gd")
const Icon = preload("res://ui/item_icon.gd")
const DefaultTheme = preload("res://ui/theme/theme.gd")
const TYPES = ["default_weapon", "default_armor", "default_potion", "default_scroll", "default_food", "default_ingredient", "default_book", "default_key", "default_misc", "default_spell"]
var failures: Array[String] = []

func _initialize() -> void:
	call_deferred("ui_24_every_category_and_item_type_has_theme_art")

func check(condition: bool, message: String) -> void:
	if not condition:
		failures.append(message)
		push_error("UI-24: " + message)

func has_art(icon: Control) -> bool:
	return (not icon.meshes.is_empty() or icon.texture != null) and icon.art_source != "builtin"

func ui_24_every_category_and_item_type_has_theme_art() -> void:
	var cache := OS.get_environment("SOE_UI_24_CACHE")
	var menu := Menu.new()
	var data: Array = []
	for index in range(TYPES.size()):
		var kind: String = Menu.INVENTORY_CATEGORIES[mini(index + 1, 9)]
		data.append({"id": str(index), "name": kind + " sample", "category": kind, "icon": TYPES[index], "weight": 1.0, "value": 5, "count": 1, "favorite": true, "equipped": true, "side": 0})
	menu.configure("inventory", data, cache)
	root.add_child(menu)
	menu.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	await process_frame
	await process_frame
	for icon_name in TYPES + ["category_all", "category_favorites", "category_gear", "category_aid", "category_magic", "marker", "frame", "background"]:
		var icon := Icon.new()
		icon.icon_name = icon_name
		icon.size = Vector2(32, 32)
		root.add_child(icon)
		check(has_art(icon), icon_name + " has theme art without an install")
		check(icon.art_source == DefaultTheme.resource_path(icon_name), icon_name + " uses declared theme resource")
		if icon_name in ["category_gear", "category_aid", "category_magic"]:
			check(icon.texture != null, icon_name + " uses Game-icons gap art")
		icon.free()
	for mode in ["inventory", "container", "barter", "favorites", "magic"]:
		menu.configure(mode, data, cache)
		menu.select_category("All")
		await process_frame
		for button in menu.categories.get_children():
			var icon = button.get_node("CategoryIcon")
			check(has_art(icon), mode + ": " + button.name + " category icon")
			if not cache.is_empty() and str(button.name) in DefaultTheme.INSTALLED_CATEGORIES:
				check(icon.art_source == "interface/skyui/icons_category_psychosteve.swf", mode + ": SkyUI wins for " + button.name)
		for button in menu.rows.get_children():
			if button is Button:
				var cells = button.get_child(0)
				check(has_art(cells.get_child(1)), mode + ": item row icon")
				check(has_art(cells.get_child(0)), mode + ": equipped marker")
				if not cache.is_empty():
					var expected := "interface/skyui/icons_category_psychosteve.swf" if cells.get_child(1).icon_name == "default_spell" else "interface/skyui/icons_item_psychosteve.swf"
					check(cells.get_child(1).art_source == expected, mode + ": SkyUI item art wins")
					check(cells.get_child(0).art_source == "interface/skyui/inventorylists.swf", mode + ": SkyUI equipped marker wins")
		for decoration_name in ["Frame", "Background"]:
			var decoration = menu.get_node(decoration_name)
			check(has_art(decoration), mode + ": " + decoration_name)
			if not cache.is_empty():
				check(decoration.art_source == "interface/skyui/itemcard.swf", "SkyUI panel art wins")
	menu.configure("inventory", data, cache)
	menu.select_category("All")
	await process_frame
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	var shades := {}
	for y in range(30, 330, 3):
		for x in range(10, 630, 3):
			shades[image.get_pixel(x, y).to_rgba32()] = true
	check(shades.size() > 20, "theme and controls rendered distinct visible pixels")
	check(image.get_pixel(630, 180).get_luminance() < 0.4, "panel contrast keeps white card text readable")
	var weapon_icon = menu.categories.get_node("Weapons/CategoryIcon")
	var origin: Vector2i = Vector2i(weapon_icon.global_position)
	var dark_pixels := 0
	for y in range(2, 18):
		for x in range(2, 18):
			if image.get_pixel(origin.x + x, origin.y + y).get_luminance() < 0.4:
				dark_pixels += 1
	check(dark_pixels >= 8, "weapon art has visible silhouette gaps rather than a helper square")
	var capture := OS.get_environment("SOE_UI_24_CAPTURE")
	if not capture.is_empty():
		check(image.save_png(capture) == OK, "window capture")
	menu.free()
	await process_frame
	if failures.is_empty():
		print("UI-24 PASS categories, item types, panels, markers, precedence and rendering")
	quit(0 if failures.is_empty() else 1)
