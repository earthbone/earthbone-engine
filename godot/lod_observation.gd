# SPDX-License-Identifier: GPL-3.0-or-later
extends RefCounted

## Compare drawn source cells, including segments whose geometry crosses cell edges.
static func overlaps(streamer: Node3D, kind: String, full: Dictionary) -> int:
	var count := 0
	for row: Dictionary in streamer.call("lod_band_surfaces", kind):
		if row.kind != kind:
			continue
		var cells: PackedStringArray = row.source_cells
		if cells.is_empty():
			for x in range(int(row.x), int(row.x) + int(row.level)):
				for y in range(int(row.y), int(row.y) + int(row.level)):
					count += int(full.has(Vector2i(x, y)))
		else:
			for text: String in cells:
				count += int(full.has(Vector2i(int(text.get_slice(":", 0)), int(text.get_slice(":", 1)))))
	return count

static func coverage(stats: Dictionary) -> Dictionary:
	var cells := {}
	for text: String in String(stats.get("covered_cells", "")).split(" ", false):
		cells[Vector2i(int(text.get_slice(":", 0)), int(text.get_slice(":", 1)))] = true
	return cells
