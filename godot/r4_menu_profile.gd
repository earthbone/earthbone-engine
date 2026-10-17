# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://r4_menu_budget.gd"
## Focused card construction diagnostic using the same seeded production menus.
func _measure(label: String, operation: Callable, minimum_rows: int = 0) -> void:
	if label.begins_with("container:"):
		return
	assert(operation.call())
	await process_frame
	var menu: Node = player.call("menu_content").get_node("ItemMenu")
	var started := Time.get_ticks_msec()
	while not menu.has_meta("card_stages_ms"):
		assert(Time.get_ticks_msec() - started < 10_000)
		await process_frame
	assert(menu.get("visible_rows").size() >= minimum_rows)
	print("R4_CARD ", label, " selected=", menu.get_meta("selection_name"),
		" description_chars=", menu.get_meta("selection_description_chars"),
		" stages_ms=", JSON.stringify(menu.get_meta("card_stages_ms")))
	player.call("close_menu")
	await process_frame
