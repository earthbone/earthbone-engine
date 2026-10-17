# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"
## Uses the launcher's loadout and the production interface converter.

var failed := false

func _configure_player() -> void:
	player.set("capture_mouse", false)

func _start() -> void:
	await super._start()
	if not loaded:
		return
	var plugins: Dictionary = streamer.call("plugin_record_memory")
	print("R4_UI plugins=", plugins.keys())
	failed = not plugins.has("SkyUI_SE.esp")
	if failed:
		quit(1)
		return
	print("R4_UI seed=", player.call("console_execute", "player.additem IronSword 1"))
	for mode in ["inventory", "container", "barter", "magic", "favorites"]:
		var opened := false
		var opening_started := Time.get_ticks_usec()
		match mode:
			"inventory": opened = player.call("open_inventory_menu")
			"container": opened = player.call("open_container_inventory", 0, 0x060359)
			"barter": opened = player.call("debug_invoke_actor_menu_native", "ShowBarterMenu", "HuldaRef", false)
			"magic": opened = player.call("open_magic_menu")
			"favorites": opened = player.call("open_favorites_menu")
		var opening_ms := (Time.get_ticks_usec() - opening_started) / 1000.0
		print("R4_UI opening mode=", mode, " ms=", opening_ms, " budget_ms=33")
		await process_frame
		var content: Node = player.call("menu_content")
		var menu: Node = content.get_node_or_null("ItemMenu") if content != null else null
		if not opened or menu == null:
			print("R4_UI mode=", mode, " opened=false")
			failed = true
			continue
		var started := Time.get_ticks_msec()
		while (str(menu.get("cache_directory")).is_empty() or menu.get("chrome_pending")
			or menu.get("sort_pending") or menu.get("selection_pending")
			or int(menu.get("next_row")) < menu.get("visible_rows").size()
			or not menu.get("art_updates").is_empty()) and Time.get_ticks_msec() - started < 120_000:
			await process_frame
		print("R4_UI presentation mode=", mode, " ms=", menu.get_meta("opening_main_thread_ms", -1.0))
		if not await _font_is_visible(menu, mode):
			quit(1)
			return
		var cache = menu.get("art_cache")
		var movie: Dictionary = cache.call("movie", "interface/skyui/inventorylists.swf")
		var sources := {}
		for child in menu.find_children("*", "Control", true, false):
			if child.has_method("_load_art"):
				var source := str(child.get("art_source"))
				sources[source] = int(sources.get(source, 0)) + 1
		print("R4_UI mode=", mode, " overlay_inventory=", not movie.is_empty(), " cache=", menu.get("cache_directory"), " art=", JSON.stringify(sources))
		var category_icon: Node = menu.get_node("Body/Categories/All/CategoryIcon")
		var selected_source := str(category_icon.get("art_source"))
		var configured_source := "interface/skyui/icons_category_psychosteve.swf"
		print("R4_UI category mode=", mode, " selected=", selected_source, " configured=", configured_source)
		failed = failed or movie.is_empty() or selected_source != configured_source
		await process_frame
		await RenderingServer.frame_post_draw
		var output := OS.get_environment("SOE_R4_OUTPUT")
		if not output.is_empty():
			DirAccess.make_dir_recursive_absolute(output)
			root.get_texture().get_image().save_png(output.path_join(mode + ".png"))
		if mode == "inventory" and not menu.get("source_rows").is_empty():
			player.call("item_menu_favorite", menu.get("source_rows")[0])
		player.call("close_menu")
		await process_frame
	quit(1 if failed else 0)

func _font_is_visible(menu: Node, mode: String) -> bool:
	var viewport := SubViewport.new()
	viewport.size = Vector2i(256, 32)
	viewport.transparent_bg = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	var label := Label.new()
	label.text = "Iron Sword NAME 123"
	var menu_theme: Theme = menu.get("theme")
	assert(menu_theme != null and menu_theme.default_font != null)
	label.add_theme_font_override("font", menu_theme.default_font)
	label.add_theme_font_size_override("font_size", 12)
	viewport.add_child(label)
	for frame in range(3):
		await process_frame
	await RenderingServer.frame_post_draw
	var pixels := 0
	var image := viewport.get_texture().get_image()
	for y in range(image.get_height()):
		for x in range(image.get_width()):
			if image.get_pixel(x, y).a > 0.2:
				pixels += 1
	print("R4_UI font mode=", mode, " pixels=", pixels, " minimum=20")
	viewport.queue_free()
	return pixels > 20
