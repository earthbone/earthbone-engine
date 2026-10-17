# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree
## Identical chrome passes, while a plain panel and moved chrome fail.

const Compare = preload("res://ui/theme_compare.gd")

func _initialize() -> void:
	var reference := Image.create(640, 360, false, Image.FORMAT_RGBA8)
	reference.fill(Color.BLACK)
	for region: Rect2 in Compare.REGIONS.values():
		var rect := Rect2i(region.position * Vector2(640, 360), region.size * Vector2(640, 360))
		reference.fill_rect(rect.grow(-3), Color.WHITE)
		reference.fill_rect(rect.grow(-5), Color.BLACK)
	var plain := Image.create(640, 360, false, Image.FORMAT_RGBA8)
	plain.fill(Color.BLACK)
	assert(Compare.compare(reference, reference).pass)
	assert(not Compare.compare(plain, reference).pass)
	var moved := plain.duplicate() as Image
	moved.blit_rect(reference, Rect2i(0, 0, 628, 360), Vector2i(12, 0))
	assert(not Compare.compare(moved, reference).pass)
	var capture_path := OS.get_environment("SOE_UI_30_CAPTURE")
	var reference_path := OS.get_environment("SOE_UI_30_REFERENCE")
	if not capture_path.is_empty() and not reference_path.is_empty():
		var result := Compare.compare(Image.load_from_file(capture_path), Image.load_from_file(reference_path))
		print(JSON.stringify(result))
		quit(0 if result.pass else 1)
		return
	print("UI-30 COMPARE PASS")
	quit(0)
