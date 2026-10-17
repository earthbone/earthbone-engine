# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree
## Import resolution is exercised through the icon presenter with local ID collisions.

const Cache = preload("res://ui/interface_cache.gd")
const Icon = preload("res://ui/item_icon.gd")
const Menu = preload("res://ui/item_menu.gd")
const Library = preload("res://ui/item_art_library.gd")

func _initialize() -> void:
	call_deferred("run")

func write_json(path: String, value: Dictionary) -> void:
	var file := FileAccess.open(path, FileAccess.WRITE)
	assert(file != null)
	file.store_string(JSON.stringify(value))

func part(character: int) -> Dictionary:
	return {"character": character, "transform": [1, 0, 0, 1, 3, 4]}

func run() -> void:
	var directory := OS.get_environment("SOE_UI_20_IMPORT_FIXTURE")
	assert(not directory.is_empty())
	for folder in ["root", "card", "cycle", "fonts", "icons"]:
		assert(DirAccess.make_dir_recursive_absolute(directory.path_join(folder)) == OK)
	write_json(directory.path_join("manifest.json"), {"movies": {
		"interface/inventorymenu.swf": "root/movie.tres",
		"interface/skyui/itemcard.swf": "card/movie.tres",
		"interface/cycle.swf": "cycle/movie.tres",
		"interface/escape.swf": "../outside/movie.tres",
		"interface/fonts.swf": "fonts/movie.tres",
		"interface/icons_aaa.swf": "icons/movie.tres",
		"interface/icons_fixture.swf": "icons/movie.tres",
	}})
	var font_config := FileAccess.open(directory.path_join("fontconfig.txt"), FileAccess.WRITE)
	font_config.store_string('fontlib "FONTS.SWF"\nmap "$EverywhereMediumFont" = "Fixture Face" Normal\nmap "$BoldFont" = "Fixture Face" Bold\nmap "$ItalicFont" = "Fixture Face" Italic\n')
	font_config.close()
	write_json(directory.path_join("fonts/movie.json"), {"fonts": {
		"2": {"name": "Fixture Face"}, "3": {"name": "Fixture Face"}, "4": {"name": "Fixture Face"},
	}})
	for character in [2, 3, 4]:
		var font := FontFile.new()
		font.font_name = "Fixture Face"
		font.font_weight = 700 if character == 3 else 400
		font.set_font_style(TextServer.FONT_ITALIC if character == 4 else (TextServer.FONT_BOLD if character == 3 else 0))
		assert(ResourceSaver.save(font, directory.path_join("fonts/font-%d.tres" % character)) == OK)
	write_json(directory.path_join("root/movie.json"), {
		"symbols": {"Root": 0, "Card": 7, "Loop": 8},
		"imports": {"SkyUI/ItemCard.SWF": {"Card": 7}, "cycle.swf": {"Loop": 8}},
		"meshes": {"1": {}},
		"timelines": {"0": {"frames": [{"display_list": [part(7)]}]}},
	})
	var background := part(1)
	background.name = "background"
	write_json(directory.path_join("card/movie.json"), {
		"symbols": {"Card": 9},
		"timelines": {"9": {"frames": [{"display_list": [background]}]}},
	})
	write_json(directory.path_join("cycle/movie.json"), {
		"symbols": {"Loop": 4}, "imports": {"inventorymenu.swf": {"Loop": 4}},
	})
	write_json(directory.path_join("icons/movie.json"), {
		"imports": {"skyui/itemcard.swf": {"Card": 1}},
		"timelines": {"5": {"frames": [{"labels": ["default_misc"], "display_list": [part(1)]}]}},
	})
	var mesh := ArrayMesh.new()
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = PackedVector3Array([Vector3(0, 0, 0), Vector3(20, 0, 0), Vector3(0, 20, 0)])
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	mesh.set_meta("swf_bounds", Rect2(0, 0, 20, 20))
	assert(ResourceSaver.save(mesh, directory.path_join("card/shape-1.tres")) == OK)
	var cache := Cache.new(directory)
	assert(cache.font("$EverywhereMediumFont").resource_path == directory.path_join("fonts/font-2.tres"))
	assert(cache.font("$BoldFont").resource_path == directory.path_join("fonts/font-3.tres"))
	assert(cache.font("$ItalicFont").resource_path == directory.path_join("fonts/font-4.tres"))
	assert(cache.font("$Missing") == null)
	assert(cache.symbol("interface/inventorymenu.swf", "Loop").is_empty())
	assert(cache.symbol("interface/inventorymenu.swf", "Missing").is_empty())
	assert(cache.movie("interface/escape.swf").is_empty())
	assert(cache.movie("interface/skyui/../INVENTORYMENU.SWF").path == "interface/inventorymenu.swf")
	var icon := Icon.new()
	icon.cache_directory = directory
	icon.cache_movie = "inventorymenu.swf"
	icon.cache_symbol = "Root"
	icon.size = Vector2(100, 100)
	root.add_child(icon)
	assert(icon.meshes.size() == 1)
	assert(icon.meshes[0].mesh.resource_path == directory.path_join("card/shape-1.tres"))
	assert(icon.meshes[0].transform.origin == Vector2(6, 8))
	assert(icon.art_bounds == Rect2(6, 8, 20, 20))
	var imported := Icon.new()
	imported.cache_directory = directory
	imported.cache_movie = "inventorymenu.swf"
	imported.cache_symbol = "Card"
	imported.cache_instance = "background"
	imported.position = Vector2(120, 0)
	imported.size = Vector2(100, 100)
	root.add_child(imported)
	assert(imported.meshes.size() == 1)
	assert(imported.meshes[0].mesh == icon.meshes[0].mesh)
	assert(imported.meshes[0].transform.origin == Vector2(3, 4))
	var menu := Menu.new()
	root.add_child(menu)
	var items := [{"id": "1", "name": "Fixture item", "category": "Misc", "icon": "default_misc", "side": 0}]
	menu.configure("inventory", items, directory, {"item": "INTERFACE\\ICONS_FIXTURE.SWF"})
	var item_icon = menu.rows.get_child(0).get_child(0).get_child(1)
	assert(item_icon.art_source == "interface/icons_fixture.swf")
	var manifest := directory.path_join("manifest.json")
	assert(DirAccess.rename_absolute(manifest, manifest + ".retained") == OK)
	menu.replace_rows(items)
	item_icon = menu.rows.get_child(0).get_child(0).get_child(1)
	assert(item_icon.art_source == "interface/icons_fixture.swf", "refresh must retain the published generation without rereading its manifest")
	assert(DirAccess.rename_absolute(manifest + ".retained", manifest) == OK)
	menu.set_art_cache("")
	item_icon = menu.rows.get_child(0).get_child(0).get_child(1)
	assert(item_icon.art_source.begins_with("res://ui/theme/"), "changing generation must discard the old installed art")
	menu.queue_free()
	await process_frame
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	assert(image.get_pixel(30, 30).r > 0.5)
	icon.queue_free()
	imported.queue_free()
	await process_frame
	await _exercise_library(directory)
	print("UI-20 IMPORT PASS")
	quit(0)

func _exercise_library(directory: String) -> void:
	var library := Library.new()
	var sources := {"item": "interface/icons_fixture.swf"}
	library.request(directory, sources)
	root.add_child(library)
	var started := Time.get_ticks_msec()
	while library.cache.directory != directory or not library.cache.prepared_resources:
		assert(Time.get_ticks_msec() - started < 10_000, "resource worker must finish")
		await process_frame
	var prepared = library.cache
	assert(prepared.font("$BoldFont").resource_path == directory.path_join("fonts/font-3.tres"))
	var hidden := ["manifest.json", "icons/movie.json", "card/movie.json", "fonts/movie.json"]
	for relative in hidden:
		var path := directory.path_join(relative)
		assert(DirAccess.rename_absolute(path, path + ".prepared") == OK)
	var items: Array = []
	for index in range(128):
		items.append({"id": str(index), "name": "Fixture item %03d" % index,
			"category": "Misc", "icon": "default_misc", "side": 0})
	for opening in range(2):
		var opening_started := Time.get_ticks_usec()
		var menu := Menu.new()
		menu.configure("inventory", items, directory, sources, library)
		root.add_child(menu)
		var opening_ms := (Time.get_ticks_usec() - opening_started) / 1000.0
		print("UI-20 ASYNC measured opening=", opening, " ms=", opening_ms)
		assert(opening_ms < 33.0, "prepared menu opening must fit the frame budget")
		assert(menu.visible_rows.size() == 128)
		started = Time.get_ticks_msec()
		while menu.next_row < 128:
			assert(Time.get_ticks_msec() - started < 10_000, "all rows must finish")
			await process_frame
		assert(menu.rows.get_child_count() == 128)
		assert(float(menu.get_meta("chrome_build_ms", 0.0)) < 33.0)
		assert(float(menu.get_meta("sort_build_ms", 0.0)) < 33.0)
		assert(float(menu.get_meta("rows_batch_max_ms", 0.0)) < 33.0)
		assert(menu.rows.get_child(0).get_child(0).get_child(1).art_source == "interface/icons_fixture.swf")
		assert(menu.art_cache == prepared, "reopening retains the player's generation")
		print("UI-20 ASYNC opening=", opening, " ms=", opening_ms, " rows=", menu.next_row,
			" batch_max_ms=", menu.get_meta("rows_batch_max_ms", 0.0))
		menu.queue_free()
		await process_frame
	for relative in hidden:
		var path := directory.path_join(relative)
		assert(DirAccess.rename_absolute(path + ".prepared", path) == OK)
	library.queue_free()
	await process_frame
