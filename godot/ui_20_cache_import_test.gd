# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree
## Import resolution is exercised through the icon presenter with local ID collisions.

const Cache = preload("res://ui/interface_cache.gd")
const Icon = preload("res://ui/item_icon.gd")

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
	for folder in ["root", "card", "cycle", "fonts"]:
		assert(DirAccess.make_dir_recursive_absolute(directory.path_join(folder)) == OK)
	write_json(directory.path_join("manifest.json"), {"movies": {
		"interface/inventorymenu.swf": "root/movie.tres",
		"interface/skyui/itemcard.swf": "card/movie.tres",
		"interface/cycle.swf": "cycle/movie.tres",
		"interface/escape.swf": "../outside/movie.tres",
		"interface/fonts.swf": "fonts/movie.tres",
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
	await process_frame
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	assert(image.get_pixel(30, 30).r > 0.5)
	icon.queue_free()
	imported.queue_free()
	await process_frame
	print("UI-20 IMPORT PASS")
	quit(0)
