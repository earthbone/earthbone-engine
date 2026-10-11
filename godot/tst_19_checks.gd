# SPDX-License-Identifier: GPL-3.0-or-later
extends RefCounted
## TST-19 thresholds fixed before installed runs. Images follow Godot's
## RenderingServer frame_post_draw and Image sRGB conversion contracts.

const FRAME_MS := 50.0
const STALL_MS := 2000.0
const WARMUP_S := 10.0
const PIXEL_DELTA := 0.025
const MIN_DRAWN_PIXELS := 4
const MAX_HEIGHT_UNITS := 512.0
const COMPASS_ERROR_PX := 0.15

static func rgb_distance(a: Color, b: Color) -> float:
	return maxf(absf(a.r - b.r), maxf(absf(a.g - b.g), absf(a.b - b.b)))

static func difference(image: Image, control: Image, area: Rect2i) -> int:
	var changed := 0
	var clipped := area.intersection(Rect2i(Vector2i.ZERO, image.get_size()))
	for y in range(clipped.position.y, clipped.end.y):
		for x in range(clipped.position.x, clipped.end.x):
			if rgb_distance(image.get_pixel(x, y), control.get_pixel(x, y)) > PIXEL_DELTA:
				changed += 1
	return changed

static func additive(image: Image, control: Image, area: Rect2i) -> Dictionary:
	var bright := 0
	var dark := 0
	var unchanged := 0
	var clipped := area.intersection(Rect2i(Vector2i.ZERO, image.get_size()))
	for y in range(clipped.position.y, clipped.end.y):
		for x in range(clipped.position.x, clipped.end.x):
			var a := image.get_pixel(x, y).srgb_to_linear()
			var b := control.get_pixel(x, y).srgb_to_linear()
			if minf(a.r - b.r, minf(a.g - b.g, a.b - b.b)) < -PIXEL_DELTA:
				dark += 1
			elif rgb_distance(a, b) > PIXEL_DELTA:
				bright += 1
			else:
				unchanged += 1
	return {"bright_pixels": bright, "dark_pixels": dark, "transmitted_pixels": unchanged}

static func plain_white(image: Image, control: Image) -> int:
	var count := 0
	for y in range(image.get_height()):
		for x in range(image.get_width()):
			var a := image.get_pixel(x, y)
			if minf(a.r, minf(a.g, a.b)) >= 0.92 and maxf(a.r, maxf(a.g, a.b)) - minf(a.r, minf(a.g, a.b)) <= 0.025 and rgb_distance(a, control.get_pixel(x, y)) > PIXEL_DELTA:
				count += 1
	return count

static func has_albedo(material: Material) -> bool:
	if material is BaseMaterial3D:
		return material.get_texture(BaseMaterial3D.TEXTURE_ALBEDO) != null
	if material is ShaderMaterial:
		for uniform: Dictionary in material.shader.get_shader_uniform_list():
			if "albedo" in str(uniform.name) or "diffuse" in str(uniform.name):
				if material.get_shader_parameter(uniform.name) is Texture2D:
					return true
	return false

static func skyrim_to_godot(point: Vector3) -> Vector3:
	return Vector3(point.x, point.z, -point.y) / 70.0

static func bounds_visible(camera: Camera3D, bounds: AABB) -> bool:
	if bounds.has_point(camera.global_position):
		return true
	for plane: Plane in camera.get_frustum():
		var all_outside := true
		for index in range(8):
			if plane.distance_to(bounds.get_endpoint(index)) <= 0.0:
				all_outside = false
				break
		if all_outside:
			return false
	return true

static func projected(camera: Camera3D, bounds: AABB) -> Rect2i:
	var result := Rect2()
	var first := true
	for index in range(8):
		var point := bounds.get_endpoint(index)
		if camera.is_position_behind(point):
			continue
		var pixel := camera.unproject_position(point)
		if first:
			result = Rect2(pixel, Vector2.ZERO)
			first = false
		else:
			result = result.expand(pixel)
	return Rect2i(result.grow(2.0)) if not first else Rect2i()

static func pose_changed(before: Array, after: Array) -> bool:
	if before.is_empty() or before.size() != after.size():
		return false
	for index in range(before.size()):
		if not before[index].is_equal_approx(after[index]):
			return true
	return false

static func compass_offset(node: Node) -> Variant:
	if node == null:
		return null
	for property: Dictionary in node.get_property_list():
		if property.name == "scroll_offset":
			return node.get("scroll_offset")
		if property.name == "scroll_horizontal":
			return float(node.get("scroll_horizontal"))
	if node.has_meta("strip_offset"):
		return float(node.get_meta("strip_offset"))
	return null

static func self_test() -> Array[String]:
	var failures: Array[String] = []
	var control := Image.create(8, 8, false, Image.FORMAT_RGBA8)
	control.fill(Color(0.3, 0.3, 0.3))
	var shown := control.duplicate() as Image
	shown.set_pixel(3, 3, Color(0.8, 0.4, 0.3))
	var area := Rect2i(0, 0, 8, 8)
	var result := additive(shown, control, area)
	if result.bright_pixels != 1 or result.dark_pixels != 0 or result.transmitted_pixels != 63:
		failures.append("additive fire")
	shown.set_pixel(4, 4, Color.BLACK)
	if additive(shown, control, area).dark_pixels != 1:
		failures.append("opaque hull detection")
	shown.fill(Color.WHITE)
	if plain_white(shown, control) != 64 or plain_white(control, control) != 0:
		failures.append("white object contribution")
	if difference(control, control, area) != 0 or difference(shown, control, area) != 64:
		failures.append("blank render detection")
	if pose_changed([], []) or pose_changed([Transform3D.IDENTITY], [Transform3D.IDENTITY]) or not pose_changed([Transform3D.IDENTITY], [Transform3D(Basis.IDENTITY, Vector3.ONE)]):
		failures.append("frozen or absent pose detection")
	return failures
