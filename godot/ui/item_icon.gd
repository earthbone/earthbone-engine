# SPDX-License-Identifier: GPL-3.0-or-later
extends Control
## Installed UI-19 art takes precedence over the UI-24 default theme.

const DefaultTheme = preload("res://ui/theme/theme.gd")
const InterfaceCache = preload("res://ui/interface_cache.gd")

const UiTrace = preload("res://ui/trace.gd")

var icon_name := "default_misc"
var cache_directory := ""
var cache_icon_name := ""
var cache_movie := ""
var cache_symbol := ""
var cache_instance := ""
var stretch_art := false
var art_color := Color.WHITE
## Mirrors installed art on the y axis, used by the descending sort arrow.
var flipped := false
var meshes: Array = []
var art_bounds := Rect2()
var art_source := "builtin"
var texture: Texture2D
var cache

func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	_load_art()
	if meshes.is_empty():
		_load_theme()
	resized.connect(queue_redraw)
	queue_redraw()

func _load_art() -> void:
	var _span = UiTrace.begin("ui.icon.art." + icon_name)
	if cache_directory.is_empty():
		return
	if cache == null or cache.directory != cache_directory:
		cache = InterfaceCache.new(cache_directory)
	var key := JSON.stringify([icon_name, cache_movie, cache_symbol, cache_instance, cache_icon_name])
	if cache.presentations.has(key):
		var prepared: Dictionary = cache.presentations[key]
		if not prepared.is_empty():
			meshes = prepared.meshes.duplicate()
			art_bounds = prepared.bounds
			art_source = prepared.source
		return
	var selected_movie := cache_movie.replace("\\", "/").to_lower()
	var movie_paths: Array = []
	if selected_movie.begins_with("interface/"):
		var requested: Dictionary = cache.movie(selected_movie)
		if requested.is_empty():
			return
		selected_movie = requested.path
		movie_paths = [selected_movie]
	else:
		movie_paths = cache.movies.keys()
		# A same-named SkyUI movie wins over the vanilla inventory component,
		# and NORDIC replaces that same path, so installed art decides the look.
		movie_paths.sort_custom(func(a, b): return str(a).begins_with("interface/skyui/") and not str(b).begins_with("interface/skyui/"))
	for movie_path in movie_paths:
		if not selected_movie.is_empty() and not str(movie_path).ends_with(selected_movie):
			continue
		if cache_movie.is_empty() and "icon" not in str(movie_path).to_lower():
			continue
		var source: Dictionary = cache.movie(str(movie_path))
		if source.is_empty():
			continue
		var metadata: Dictionary = source.data
		if not cache_symbol.is_empty():
			var definition: Dictionary = cache.symbol(str(movie_path), cache_symbol)
			if not definition.is_empty():
				source = definition.movie
				metadata = source.data
				var character: int = definition.character
				if cache_instance.is_empty():
					_collect(source, [{"character": character, "transform": [1, 0, 0, 1, 0, 0]}], Transform2D.IDENTITY, 0)
				else:
					var timeline = metadata.get("timelines", {}).get(str(int(character)), {})
					for frame in timeline.get("frames", []):
						var instances: Array = frame.get("display_list", []).filter(func(part): return part.get("name", "") == cache_instance)
						_collect(source, instances, Transform2D.IDENTITY, 0)
						if not meshes.is_empty():
							break
				if not meshes.is_empty():
					art_source = str(movie_path)
					_remember_art(key)
					return
			continue
		var label := cache_icon_name if not cache_icon_name.is_empty() else icon_name
		if label == "default_spell":
			label = "mag_all"
		for frame in cache.labeled_frames(source, label):
			_collect(source, frame.display_list, Transform2D.IDENTITY, 0)
			if not meshes.is_empty():
				art_source = str(movie_path)
				_remember_art(key)
				return
	cache.presentations[key] = {}

func _remember_art(key: String) -> void:
	cache.presentations[key] = {"meshes": meshes.duplicate(), "bounds": art_bounds, "source": art_source}

func use_cache(prepared) -> void:
	cache = prepared
	cache_directory = prepared.directory
	meshes = []
	texture = null
	art_bounds = Rect2()
	art_source = "builtin"
	_load_art()
	if meshes.is_empty():
		_load_theme()
	queue_redraw()

func _load_theme() -> void:
	if icon_name == "sort_arrow":
		return
	var _span = UiTrace.begin("ui.icon.theme." + icon_name)
	var path := ""
	var value: Resource
	if cache != null and cache.prepared_resources:
		var prepared: Dictionary = cache.fallbacks.get(icon_name, cache.fallbacks.get("default_misc", {}))
		if prepared.is_empty():
			return
		path = prepared.path
		value = prepared.resource
	else:
		path = DefaultTheme.resource_path(icon_name)
		if path.is_empty() or not FileAccess.file_exists(path):
			return
	if value == null and path.ends_with(".svg") and (cache == null or not cache.prepared_resources):
		var pixels := Image.new()
		if pixels.load_svg_from_string(FileAccess.get_file_as_string(path)) == OK:
			texture = ImageTexture.create_from_image(pixels)
	else:
		if value == null:
			value = cache.resource(path) if cache != null else load(path)
		if value is Mesh:
			meshes.append({"mesh": value, "transform": Transform2D.IDENTITY})
			art_bounds = value.get_meta("swf_bounds", Rect2())
		elif value is Texture2D:
			texture = value
	if not meshes.is_empty() or texture != null:
		art_source = path

func _collect(source: Dictionary, instances: Array, transform: Transform2D, depth: int) -> void:
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
		var resolved: Dictionary = cache.character_source(source, int(instance.character))
		if resolved.is_empty():
			continue
		var target: Dictionary = resolved.movie
		var character: int = resolved.character
		var shape_path := str(target.directory).path_join("shape-%d.tres" % character)
		var mesh = cache.resource(shape_path)
		if mesh is Mesh:
			meshes.append({"mesh": mesh, "transform": local})
			var bounds: Rect2 = local * mesh.get_meta("swf_bounds", Rect2())
			art_bounds = bounds if art_bounds.size == Vector2.ZERO else art_bounds.merge(bounds)
		else:
			var timeline = target.data.get("timelines", {}).get(str(character), {})
			if not timeline.get("frames", []).is_empty():
				_collect(target, timeline.frames[0].display_list, local, depth + 1)

static var painted_meshes: Dictionary = {}
static var paint_pattern: RegEx

## draw_mesh ignores surface materials, so a converted shape would draw solid
## white. This copy bakes each surface's SWF paint into vertex colours: the
## solid colour, or the mean of the stops for a gradient. A bitmap paint has no
## flat colour and stays transparent.
static func painted_mesh(source: Mesh) -> Mesh:
	var id := source.get_instance_id()
	if painted_meshes.has(id):
		return painted_meshes[id]
	if paint_pattern == null:
		paint_pattern = RegEx.create_from_string("vec4\\(([-0-9.e]+),([-0-9.e]+),([-0-9.e]+),([-0-9.e]+)\\)")
	var result := ArrayMesh.new()
	for surface in range(source.get_surface_count()):
		var arrays := source.surface_get_arrays(surface)
		var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
		var paint := Color(1, 1, 1, 0)
		var material := source.surface_get_material(surface) as ShaderMaterial
		if material != null and material.shader != null and not "texture(bitmap" in material.shader.code:
			var sum := Vector4.ZERO
			var count := 0
			for found in paint_pattern.search_all(material.shader.code):
				sum += Vector4(float(found.get_string(1)), float(found.get_string(2)), float(found.get_string(3)), float(found.get_string(4)))
				count += 1
			if count > 0:
				sum /= count
				paint = Color(sum.x, sum.y, sum.z, sum.w)
		var colors := PackedColorArray()
		colors.resize(vertices.size())
		colors.fill(paint)
		arrays[Mesh.ARRAY_COLOR] = colors
		result.add_surface_from_arrays(source.surface_get_primitive_type(surface), arrays)
	result.set_meta("swf_bounds", source.get_meta("swf_bounds", Rect2()))
	painted_meshes[id] = result
	return result

func _draw() -> void:
	if texture != null:
		draw_texture_rect(texture, Rect2(Vector2.ONE * 2, size - Vector2.ONE * 4), false)
		return
	if not meshes.is_empty() and art_bounds.size.x > 0 and art_bounds.size.y > 0:
		var scale_xy := (size - Vector2.ONE * 4) / art_bounds.size
		if not stretch_art:
			scale_xy = Vector2.ONE * minf(scale_xy.x, scale_xy.y)
		if flipped:
			scale_xy.y = -scale_xy.y
		var fit := Transform2D.IDENTITY.scaled(scale_xy)
		fit.origin = size * 0.5 - art_bounds.get_center() * scale_xy
		# Installed SWF art keeps the colours its author drew. The tint only
		# darkens the bundled fallback shapes.
		var installed := art_source.begins_with("interface/")
		var tint := Color.WHITE if installed else art_color
		for part in meshes:
			draw_mesh(painted_mesh(part.mesh) if installed else part.mesh, null, fit * part.transform, tint)
		return
	if icon_name == "sort_arrow":
		var half := minf(size.x, size.y) * 0.5
		var mid := size * 0.5
		var tip := -half if not flipped else half
		draw_colored_polygon(PackedVector2Array([mid + Vector2(0, tip * 0.6), mid + Vector2(half, -tip * 0.6), mid + Vector2(-half, -tip * 0.6)]), Color(0.9, 0.92, 0.93))
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
