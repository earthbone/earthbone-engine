# SPDX-License-Identifier: GPL-3.0-or-later
extends Control
## Installed UI-19 art takes precedence over the UI-24 default theme.

const DefaultTheme = preload("res://ui/theme/theme.gd")

var icon_name := "default_misc"
var cache_directory := ""
var cache_icon_name := ""
var cache_movie := ""
var cache_symbol := ""
var cache_instance := ""
var stretch_art := false
var art_color := Color.WHITE
var meshes: Array = []
var art_bounds := Rect2()
var art_source := "builtin"
var texture: Texture2D

func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	_load_art()
	if meshes.is_empty():
		_load_theme()
	resized.connect(queue_redraw)
	queue_redraw()

func _load_art() -> void:
	if cache_directory.is_empty():
		return
	var manifest = JSON.parse_string(FileAccess.get_file_as_string(cache_directory.path_join("manifest.json")))
	if not manifest is Dictionary:
		return
	for movie_path in manifest.get("movies", {}):
		if not cache_movie.is_empty() and not str(movie_path).ends_with(cache_movie):
			continue
		if cache_movie.is_empty() and "icon" not in str(movie_path).to_lower():
			continue
		var directory := cache_directory.path_join(str(manifest.movies[movie_path])).get_base_dir()
		var metadata = JSON.parse_string(FileAccess.get_file_as_string(directory.path_join("movie.json")))
		if not metadata is Dictionary:
			continue
		if not cache_symbol.is_empty():
			var character = metadata.get("symbols", {}).get(cache_symbol)
			if character != null:
				if cache_instance.is_empty():
					_collect(metadata, directory, [{"character": character, "transform": [1, 0, 0, 1, 0, 0]}], Transform2D.IDENTITY, 0)
				else:
					var timeline = metadata.get("timelines", {}).get(str(int(character)), {})
					for frame in timeline.get("frames", []):
						var instances: Array = frame.get("display_list", []).filter(func(part): return part.get("name", "") == cache_instance)
						_collect(metadata, directory, instances, Transform2D.IDENTITY, 0)
						if not meshes.is_empty():
							break
				if not meshes.is_empty():
					art_source = str(movie_path)
					return
			continue
		var label := cache_icon_name if not cache_icon_name.is_empty() else icon_name
		if label == "default_spell":
			label = "mag_all"
		for timeline in metadata.get("timelines", {}).values():
			for frame in timeline.get("frames", []):
				if label in frame.get("labels", []):
					_collect(metadata, directory, frame.display_list, Transform2D.IDENTITY, 0)
					if not meshes.is_empty():
						art_source = str(movie_path)
						return

func _load_theme() -> void:
	var path := DefaultTheme.resource_path(icon_name)
	if path.is_empty() or not FileAccess.file_exists(path):
		return
	if path.ends_with(".svg"):
		var pixels := Image.new()
		if pixels.load_svg_from_string(FileAccess.get_file_as_string(path)) == OK:
			texture = ImageTexture.create_from_image(pixels)
	else:
		var resource = load(path)
		if resource is Mesh:
			meshes.append({"mesh": resource, "transform": Transform2D.IDENTITY})
			art_bounds = resource.get_meta("swf_bounds", Rect2())
		elif resource is Texture2D:
			texture = resource
	if not meshes.is_empty() or texture != null:
		art_source = path

func _collect(metadata: Dictionary, directory: String, instances: Array, transform: Transform2D, depth: int) -> void:
	if depth >= 16:
		return
	for instance in instances:
		if not instance.get("visible", true):
			continue
		var color: Array = instance.get("color_transform", [1, 1, 1, 1, 0, 0, 0, 0])
		if float(color[3]) + float(color[7]) <= 0:
			continue
		var t: Array = instance.transform
		var local := transform * Transform2D(Vector2(t[0], t[1]), Vector2(t[2], t[3]), Vector2(t[4], t[5]))
		var shape_path := directory.path_join("shape-%d.tres" % int(instance.character))
		if FileAccess.file_exists(shape_path):
			var mesh = load(shape_path)
			if mesh is Mesh:
				meshes.append({"mesh": mesh, "transform": local})
				var bounds: Rect2 = local * mesh.get_meta("swf_bounds", Rect2())
				art_bounds = bounds if art_bounds.size == Vector2.ZERO else art_bounds.merge(bounds)
		else:
			var timeline = metadata.get("timelines", {}).get(str(int(instance.character)), {})
			if not timeline.get("frames", []).is_empty():
				_collect(metadata, directory, timeline.frames[0].display_list, local, depth + 1)

func _draw() -> void:
	if texture != null:
		draw_texture_rect(texture, Rect2(Vector2.ONE * 2, size - Vector2.ONE * 4), false)
		return
	if not meshes.is_empty() and art_bounds.size.x > 0 and art_bounds.size.y > 0:
		var scale_xy := (size - Vector2.ONE * 4) / art_bounds.size
		if not stretch_art:
			scale_xy = Vector2.ONE * minf(scale_xy.x, scale_xy.y)
		var fit := Transform2D.IDENTITY.scaled(scale_xy)
		fit.origin = size * 0.5 - art_bounds.get_center() * scale_xy
		for part in meshes:
			draw_mesh(part.mesh, null, fit * part.transform, art_color)
		return
	var color := Color(0.78, 0.82, 0.83)
	var center := size * 0.5
	var radius := minf(size.x, size.y) * 0.35
	if "weapon" in icon_name:
		draw_line(center + Vector2(-radius, radius), center + Vector2(radius, -radius), color, 2.0)
		draw_line(center + Vector2(-radius, 0), center + Vector2(0, radius), color, 2.0)
	elif "armor" in icon_name:
		draw_colored_polygon(PackedVector2Array([center + Vector2(-radius, -radius), center + Vector2(radius, -radius), center + Vector2(radius * 0.7, radius * 0.5), center + Vector2(0, radius), center + Vector2(-radius * 0.7, radius * 0.5)]), color)
	elif "potion" in icon_name or "food" in icon_name:
		draw_circle(center + Vector2(0, radius * 0.3), radius * 0.7, color)
		draw_rect(Rect2(center + Vector2(-radius * 0.3, -radius), Vector2(radius * 0.6, radius)), color)
	elif "key" in icon_name:
		draw_arc(center - Vector2(radius * 0.4, 0), radius * 0.5, 0, TAU, 16, color, 2.0)
		draw_line(center, center + Vector2(radius, 0), color, 2.0)
		draw_line(center + Vector2(radius, 0), center + Vector2(radius, radius * 0.5), color, 2.0)
	elif "book" in icon_name or "scroll" in icon_name:
		draw_rect(Rect2(center - Vector2(radius, radius), Vector2(radius * 2, radius * 2)), color, false, 1.5)
		draw_line(center + Vector2(0, -radius), center + Vector2(0, radius), color, 1.5)
	elif "ingredient" in icon_name:
		draw_colored_polygon(PackedVector2Array([center + Vector2(-radius, radius), center + Vector2(-radius * 0.5, -radius * 0.5), center + Vector2(radius, -radius), center + Vector2(radius * 0.5, radius * 0.5)]), color)
	else:
		draw_colored_polygon(PackedVector2Array([center + Vector2(0, -radius), center + Vector2(radius, 0), center + Vector2(0, radius), center + Vector2(-radius, 0)]), color)
