# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree

## Compares the SkyUI and NORDIC captures of r4_ui_play.gd. NORDIC on top of
## SkyUI must change panel art, not only the category icons, so a pair whose
## differing pixels all sit inside the category column fails. Each capture must
## also stay well away from the plain fallback panel and keep the sort arrow
## small.
##
## Run: godot --path godot --headless --script r4_ui_compare.gd -- <skyui_dir> <nordic_dir>
## Headless is enough, it only reads PNG files.

const MENUS := ["inventory", "container", "barter", "magic", "favorites"]
## Category column in the 960x540 captures, with a margin for icon glow.
const ICON_AREA := Rect2i(46, 80, 112, 224)
## A real theme change touches this share of the pixels outside the icon area.
const MIN_OUTSIDE_DIFF := 0.02
const CHANNEL_DELTA := 0.04
## The header sort arrow may not cover more than this share of the header band.
const HEADER_BAND := Rect2i(160, 114, 532, 86)
const MAX_BRIGHT_HEADER := 0.04

func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	if args.size() < 2:
		print("R4_UI_COMPARE usage: <skyui_dir> <nordic_dir>")
		quit(2)
		return
	var failures: Array = []
	for menu in MENUS:
		var sky := _load(args[0], menu)
		var nordic := _load(args[1], menu)
		if sky == null or nordic == null:
			failures.append("%s: capture missing" % menu)
			continue
		if sky.get_size() != nordic.get_size():
			failures.append("%s: capture sizes differ" % menu)
			continue
		var outside := _outside_icon_diff(sky, nordic)
		print("R4_UI_COMPARE menu=%s outside_diff=%.4f" % [menu, outside])
		if outside < MIN_OUTSIDE_DIFF:
			failures.append("%s: SkyUI and NORDIC differ only in the icon area (outside diff %.4f)" % [menu, outside])
		for pair in [["skyui", sky], ["nordic", nordic]]:
			var bright := _bright_share(pair[1], HEADER_BAND)
			print("R4_UI_COMPARE menu=%s %s header_bright=%.4f" % [menu, pair[0], bright])
			if bright > MAX_BRIGHT_HEADER:
				failures.append("%s %s: header holds a large bright shape (%.4f), the sort arrow is too big" % [menu, pair[0], bright])
	print("R4_UI_COMPARE failures=%d" % failures.size())
	for failure in failures:
		print("R4_UI_COMPARE FAIL %s" % failure)
	quit(0 if failures.is_empty() else 1)

func _load(directory: String, menu: String) -> Image:
	var path := "%s/%s.png" % [directory, menu]
	if not FileAccess.file_exists(path):
		return null
	return Image.load_from_file(path)

func _outside_icon_diff(a: Image, b: Image) -> float:
	var total := 0
	var changed := 0
	for y in a.get_height():
		for x in a.get_width():
			if ICON_AREA.has_point(Vector2i(x, y)):
				continue
			total += 1
			var ca := a.get_pixel(x, y)
			var cb := b.get_pixel(x, y)
			if absf(ca.r - cb.r) > CHANNEL_DELTA or absf(ca.g - cb.g) > CHANNEL_DELTA or absf(ca.b - cb.b) > CHANNEL_DELTA:
				changed += 1
	return float(changed) / maxf(1.0, float(total))

func _bright_share(image: Image, band: Rect2i) -> float:
	var total := 0
	var bright := 0
	for y in range(band.position.y, mini(band.end.y, image.get_height())):
		for x in range(band.position.x, mini(band.end.x, image.get_width())):
			total += 1
			if image.get_pixel(x, y).get_luminance() > 0.7:
				bright += 1
	return float(bright) / maxf(1.0, float(total))
