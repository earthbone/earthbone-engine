# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"
## Opening calls use a fixed frame budget with the production art cache ready.

const OPENING_BUDGET_MS := 33.0
const CHEST_ROWS := 256
var failures := 0

func _configure_player() -> void:
	player.set("capture_mouse", false)

func _measure(label: String, operation: Callable, minimum_rows: int = 0) -> void:
	var started := Time.get_ticks_usec()
	var opened: bool = operation.call()
	var elapsed_ms := (Time.get_ticks_usec() - started) / 1000.0
	await process_frame
	var content: Node = player.call("menu_content")
	var menu: Node = content.get_node_or_null("ItemMenu") if content != null else null
	var row_count: int = menu.get("visible_rows").size() if menu != null else -1
	var presentation_ms: float = menu.get_meta("opening_main_thread_ms", -1.0) if menu != null else -1.0
	var passed := opened and presentation_ms >= 0.0 and maxf(elapsed_ms, presentation_ms) < OPENING_BUDGET_MS and row_count >= minimum_rows
	print("R4_MENU ", JSON.stringify({"label": label, "ms": elapsed_ms,
		"presentation_ms": presentation_ms, "budget_ms": OPENING_BUDGET_MS, "opened": opened, "rows": row_count,
		"minimum_rows": minimum_rows, "pass": passed}))
	if not passed:
		failures += 1
	if menu != null:
		var fill_started := Time.get_ticks_msec()
		while menu.get("chrome_pending") or menu.get("sort_pending") or menu.get("selection_pending") or int(menu.get("next_row")) < row_count:
			if Time.get_ticks_msec() - fill_started > 10_000:
				failures += 1
				printerr("R4_MENU row construction did not finish: ", label)
				break
			await process_frame
		var chrome_ms: float = menu.get_meta("chrome_build_ms", 0.0)
		var sort_ms: float = menu.get_meta("sort_build_ms", 0.0)
		var batch_ms: float = menu.get_meta("rows_batch_max_ms", 0.0)
		if chrome_ms >= OPENING_BUDGET_MS or sort_ms >= OPENING_BUDGET_MS or batch_ms >= OPENING_BUDGET_MS:
			failures += 1
		print("R4_MENU background ", label, " chrome_ms=", chrome_ms,
			" sort_ms=", sort_ms, " batch_ms=", batch_ms, " built=", menu.get("next_row"))
	player.call("close_menu")
	await process_frame

func _command(command: String) -> bool:
	var result: String = player.call("console_execute", command)
	if result.begins_with("error:"):
		printerr("R4_MENU setup ", command, ": ", result)
		failures += 1
		return false
	return true

func _start() -> void:
	await super._start()
	if not loaded:
		return
	player.call("open_inventory_menu")
	var menu: Node = player.call("menu_content").get_node("ItemMenu")
	var started := Time.get_ticks_msec()
	while str(menu.get("cache_directory")).is_empty() and Time.get_ticks_msec() - started < 120_000:
		await process_frame
	if str(menu.get("cache_directory")).is_empty():
		printerr("R4_MENU production art cache not ready")
		quit(1)
		return
	print("R4_MENU cache=", menu.get("cache_directory"))
	player.call("close_menu")
	await process_frame
	var active := {}
	for reference: Dictionary in player.call("play_path_references"):
		if reference.kind == "CONT":
			active["%d:%d" % [reference.file, reference.object]] = true
	var containers: Array = player.call("gameplay_containers")
	containers = containers.filter(func(entry: Dictionary) -> bool: return active.has("%d:%d" % [entry.file, entry.object]))
	containers.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return int(a.object) < int(b.object))
	var chest: Dictionary = {}
	for container: Dictionary in containers:
		if "chest" in str(container.name).to_lower() and chest.is_empty():
			chest = container
		_command("prid %08X" % int(container.object))
		_command("unlock")
		await _measure("container:%06X" % int(container.object), Callable(player, "open_container_inventory").bind(container.file, container.object))
	if chest.is_empty():
		printerr("R4_MENU Bannered Mare chest missing")
		quit(1)
		return
	var forms := {}
	var pattern := RegEx.create_from_string('^[A-Z0-9_]+: \\(([0-9A-F]{8})\\)')
	for kind in ["WEAP", "ARMO", "MISC", "ALCH", "INGR", "BOOK", "AMMO", "KEYM"]:
		var listing: String = player.call("console_execute", 'help "" 4 ' + kind)
		for line in listing.split("\n"):
			var match_form := pattern.search(line)
			if match_form != null:
				forms[match_form.get_string(1)] = true
	var ordered: Array = forms.keys()
	ordered.sort()
	if ordered.size() < CHEST_ROWS:
		printerr("R4_MENU insufficient carryable forms: ", ordered.size())
		quit(1)
		return
	_command("prid %08X" % int(chest.object))
	for form in ordered.slice(0, CHEST_ROWS):
		_command("additem " + str(form) + " 5")
		_command("player.additem " + str(form) + " 5")
	await process_frame
	await _measure("full_chest:%06X" % int(chest.object), Callable(player, "open_container_inventory").bind(chest.file, chest.object), CHEST_ROWS)
	await _measure("inventory", Callable(player, "open_inventory_menu"), CHEST_ROWS)
	await _measure("barter", Callable(player, "debug_invoke_actor_menu_native").bind("ShowBarterMenu", "HuldaRef", false))
	print("R4_MENU failures=", failures, " containers=", containers.size(), " chest_rows=", CHEST_ROWS)
	quit(0 if failures == 0 else 1)
