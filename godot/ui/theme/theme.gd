# SPDX-License-Identifier: GPL-3.0-or-later
extends RefCounted
## UI-24 uses approved UI-19 art resources and credited Game-icons gaps.

const MANIFEST_PATH = "res://ui/theme/manifest.json"
const CATEGORY_ICONS = {"All": "category_all", "Favorites": "category_favorites", "Weapons": "default_weapon", "Apparel": "default_armor", "Potions": "default_potion", "Scrolls": "default_scroll", "Food": "default_food", "Ingredients": "default_ingredient", "Books": "default_book", "Keys": "default_key", "Misc": "default_misc", "Gear": "category_gear", "Aid": "category_aid", "Magic": "category_magic", "Spells": "default_spell", "Lights": "default_misc", "Shouts": "default_spell"}
const INSTALLED_CATEGORIES = {"All": "inv_all", "Favorites": "cat_favorites", "Weapons": "inv_weapons", "Apparel": "inv_armor", "Potions": "inv_potions", "Scrolls": "inv_scrolls", "Food": "inv_food", "Ingredients": "inv_ingredients", "Books": "inv_books", "Keys": "inv_keys", "Misc": "inv_misc", "Spells": "mag_all"}

static func resource_path(icon_name: String) -> String:
	var manifest = JSON.parse_string(FileAccess.get_file_as_string(MANIFEST_PATH))
	if not manifest is Dictionary:
		return ""
	return str(manifest.get(icon_name, manifest.get("default_misc", "")))

static func category_icon(category: String) -> String:
	return CATEGORY_ICONS.get(category, "default_misc")

static func installed_category(category: String) -> String:
	return INSTALLED_CATEGORIES.get(category, "")

static func controls() -> Theme:
	var result := Theme.new()
	result.default_font_size = 12
	for state in ["normal", "hover", "pressed", "focus", "disabled"]:
		var style := StyleBoxFlat.new()
		style.bg_color = Color(0.03, 0.04, 0.05, 0.85)
		style.border_color = Color(0.65, 0.70, 0.73, 0.65)
		style.set_content_margin_all(3)
		if state in ["hover", "pressed", "focus"]:
			style.bg_color = Color(0.22, 0.26, 0.28, 0.85)
			style.set_border_width_all(1)
		result.set_stylebox(state, "Button", style)
	result.set_color("font_color", "Button", Color(0.82, 0.86, 0.88))
	return result
