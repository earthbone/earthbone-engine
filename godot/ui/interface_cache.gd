# SPDX-License-Identifier: GPL-3.0-or-later
extends RefCounted
## UI-19 imports and font aliases follow SkyUI's ImportAssets2 and fontconfig.

var directory := ""
var movies: Dictionary = {}
var metadata: Dictionary = {}
var font_libraries: Array[String] = []
var font_aliases: Dictionary = {}
var fonts: Dictionary = {}

func _init(cache_directory: String = "") -> void:
	directory = cache_directory
	var path := directory.path_join("manifest.json")
	if directory.is_empty() or not FileAccess.file_exists(path):
		return
	var manifest = JSON.parse_string(FileAccess.get_file_as_string(path))
	if manifest is Dictionary and manifest.get("movies") is Dictionary:
		movies = manifest.movies
	_read_font_config()

func _read_font_config() -> void:
	var path := directory.path_join("fontconfig.txt")
	if not FileAccess.file_exists(path):
		return
	var library := RegEx.create_from_string('^fontlib\\s+"([^"]+)"')
	var mapping := RegEx.create_from_string('^map\\s+"([^"]+)"\\s*=\\s*"([^"]+)"\\s+(.*)')
	for line in FileAccess.get_file_as_string(path).split("\n"):
		var text := line.strip_edges()
		var match_library := library.search(text)
		if match_library != null:
			font_libraries.append(match_library.get_string(1))
		var match_mapping := mapping.search(text)
		if match_mapping != null:
			var style := match_mapping.get_string(3).to_lower()
			font_aliases[match_mapping.get_string(1)] = {
				"family": match_mapping.get_string(2),
				"bold": "bold" in style, "italic": "italic" in style,
			}

func font(alias: String) -> Font:
	if fonts.has(alias):
		return fonts[alias]
	if not font_aliases.has(alias):
		return null
	var requested: Dictionary = font_aliases[alias]
	for library in font_libraries:
		var source := movie("interface/".path_join(library))
		if source.is_empty():
			continue
		for character in source.data.get("fonts", {}):
			var family := str(source.data.fonts[character].get("name", "")).trim_suffix(String.chr(0))
			if family != requested.family:
				continue
			var path := str(source.directory).path_join("font-%d.tres" % int(character))
			if not FileAccess.file_exists(path):
				continue
			var resource = load(path)
			if resource is FontFile and (resource.font_weight >= 700) == requested.bold and bool(resource.get_font_style() & TextServer.FONT_ITALIC) == requested.italic:
				fonts[alias] = resource
				return resource
	return null

func movie(path: String) -> Dictionary:
	path = path.replace("\\", "/").to_lower().simplify_path()
	if path in metadata:
		return metadata[path]
	if not path.begins_with("interface/") or not movies.has(path):
		return {}
	var relative := str(movies[path]).replace("\\", "/").simplify_path()
	if relative.is_absolute_path() or relative == ".." or relative.begins_with("../") or ":" in relative:
		return {}
	var folder := directory.path_join(relative).get_base_dir()
	var file := folder.path_join("movie.json")
	if not FileAccess.file_exists(file):
		return {}
	var data = JSON.parse_string(FileAccess.get_file_as_string(file))
	if not data is Dictionary:
		return {}
	var result := {"path": path, "directory": folder, "data": data}
	metadata[path] = result
	return result

func symbol(path: String, linkage: String) -> Dictionary:
	var source := movie(path)
	if source.is_empty():
		return {}
	var character = source.data.get("symbols", {}).get(linkage)
	return {} if character == null else character_source(source, int(character))

func character_source(source: Dictionary, character: int, visited: Array = []) -> Dictionary:
	if source.is_empty() or visited.size() >= 32:
		return {}
	var key := "%s:%d" % [source.path, character]
	if key in visited:
		return {}
	var data: Dictionary = source.data
	for kind in ["shape", "bitmap", "font"]:
		if FileAccess.file_exists(str(source.directory).path_join("%s-%d.tres" % [kind, character])):
			return {"movie": source, "character": character}
	for kind in ["meshes", "bitmaps", "fonts", "text_fields", "static_text", "timelines", "buttons"]:
		if data.get(kind, {}).has(str(character)):
			return {"movie": source, "character": character}
	var next := visited.duplicate()
	next.append(key)
	for url in data.get("imports", {}):
		for linkage in data.imports[url]:
			if int(data.imports[url][linkage]) != character:
				continue
			var target := movie(str(source.path).get_base_dir().path_join(str(url)))
			if target.is_empty():
				return {}
			var imported = target.data.get("symbols", {}).get(linkage)
			if imported != null:
				return character_source(target, int(imported), next)
	return {}
