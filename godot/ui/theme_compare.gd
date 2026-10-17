# SPDX-License-Identifier: GPL-3.0-or-later
extends RefCounted
## UI-30 static chrome checks keep independent region and edge thresholds.

const PIXEL_ERROR := 0.18
const PIXEL_MATCH := 0.90
const EDGE_CONTRAST := 0.08
const EDGE_MATCH := 0.85
const MIN_EDGES := 12
const REGIONS = {
	"frame": Rect2(0.01, 0.04, 0.97, 0.91),
	"list_header": Rect2(0.02, 0.12, 0.44, 0.08),
	"item_card": Rect2(0.56, 0.38, 0.40, 0.51),
	"category_bar": Rect2(0.02, 0.04, 0.44, 0.08),
}

static func _distance(a: Color, b: Color) -> float:
	return absf(a.r - b.r) + absf(a.g - b.g) + absf(a.b - b.b)

static func _edge(image: Image, x: int, y: int) -> bool:
	if x < 0 or y < 0 or x + 1 >= image.get_width() or y + 1 >= image.get_height():
		return false
	var pixel := image.get_pixel(x, y)
	return _distance(pixel, image.get_pixel(x + 1, y)) > EDGE_CONTRAST or _distance(pixel, image.get_pixel(x, y + 1)) > EDGE_CONTRAST

static func compare(actual: Image, reference: Image) -> Dictionary:
	var result := {"pass": false, "regions": {}}
	if actual == null or reference == null or actual.is_empty() or reference.is_empty():
		return result
	var expected := reference.duplicate() as Image
	expected.resize(actual.get_width(), actual.get_height(), Image.INTERPOLATE_LANCZOS)
	result.pass = true
	for name in REGIONS:
		var normalized: Rect2 = REGIONS[name]
		var rect := Rect2i(normalized.position * Vector2(actual.get_size()), normalized.size * Vector2(actual.get_size()))
		var pixels := 0
		var matching := 0
		var edges := 0
		var aligned := 0
		for y in range(rect.position.y, rect.end.y):
			for x in range(rect.position.x, rect.end.x):
				if expected.get_pixel(x, y).a < 0.05:
					continue
				pixels += 1
				if _distance(actual.get_pixel(x, y), expected.get_pixel(x, y)) <= PIXEL_ERROR:
					matching += 1
				if _edge(expected, x, y):
					edges += 1
					var found := false
					for dy in range(-1, 2):
						for dx in range(-1, 2):
							found = found or _edge(actual, x + dx, y + dy)
					if found:
						aligned += 1
		var pixel_match := float(matching) / float(pixels) if pixels > 0 else 0.0
		var edge_match := float(aligned) / float(edges) if edges > 0 else 0.0
		var passed := pixels > 0 and edges >= MIN_EDGES and pixel_match >= PIXEL_MATCH and edge_match >= EDGE_MATCH
		result.regions[name] = {"pass": passed, "pixel_match": pixel_match, "edge_match": edge_match, "reference_edges": edges}
		result.pass = result.pass and passed
	return result
