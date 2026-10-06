extends SceneTree

## UI-26 installed-data scene check for crosshair loot and ownership markers.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL UI-26: ", message)

func _frames() -> void:
	for index in range(4):
		await process_frame
	await RenderingServer.frame_post_draw

func _open(player: Node, holder: Dictionary) -> Dictionary:
	_check(player.call("quick_loot_force_container", holder.file, holder.object),
		"container could not be forced into the QuickLoot scene")
	await _frames()
	return player.call("quick_loot_snapshot")

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL UI-26: requires SKYRIM_DATA and the release extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "installed data did not load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", true)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data was not attached")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"Bannered Mare collision was not loaded")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	await _frames()
	var containers: Array = player.call("gameplay_containers")
	var take_one := {}
	var take_all := {}
	var owned := {}
	var selectable := {}
	var empty_or_spent := {}
	for holder in containers:
		var state := await _open(player, holder)
		var count := int(state.get("count", 0))
		var total := int(state.get("total", 0))
		if count > 0 and take_one.is_empty():
			take_one = holder
		if total > 1 and take_all.is_empty():
			take_all = holder
		if int(state.get("stealing", 0)) > 0 and owned.is_empty():
			owned = holder
		if count > 1 and selectable.is_empty():
			selectable = holder
		if count == 0 and empty_or_spent.is_empty():
			empty_or_spent = holder
		player.call("quick_loot_clear_forced_container")
		await _frames()
	_check(not take_one.is_empty(), "no nonempty loaded container was available")
	_check(not owned.is_empty(), "installed cell had no owned loot row to test the steal marker")
	if not owned.is_empty():
		await _open(player, owned)
		var rows: VBoxContainer = player.find_child("QuickLootRows", true, false) as VBoxContainer
		var marker_found := false
		if rows != null:
			for row in rows.get_children():
				if row is Button and row.text.contains("[Steal]"):
					marker_found = true
		_check(marker_found, "owned row did not render its steal marker")
	if not selectable.is_empty():
		await _open(player, selectable)
		var down := InputEventKey.new()
		down.keycode = KEY_DOWN
		down.pressed = true
		Input.parse_input_event(down)
		await _frames()
		_check(int(player.call("quick_loot_snapshot").get("selected", 0)) == 1,
			"Down did not scroll the selected loot row")
		var sort := InputEventKey.new()
		sort.keycode = KEY_R
		sort.pressed = true
		Input.parse_input_event(sort)
		await _frames()
		_check(player.call("quick_loot_snapshot").get("sort", "") == "Value",
			"R did not cycle the loot sort")
	else:
		_check(false, "installed cell had no multi-row container for scroll and sort input")
	var target := take_one if not take_one.is_empty() else take_all
	if not target.is_empty():
		await _open(player, target)
		var selected: bool = player.call("quick_loot_select", 0)
		_check(selected, "row selection failed")
		var rows: VBoxContainer = player.find_child("QuickLootRows", true, false) as VBoxContainer
		var icon_found := false
		if rows != null and rows.get_child_count() > 0:
			var first_row := rows.get_child(0) as Button
			icon_found = first_row != null and ["⚔", "◇", "✚", "•"].any(func(icon): return first_row.text.contains(icon))
		_check(icon_found, "loot row did not show a category icon")
		_check(player.call("preview_inventory_item", 0, 0x12eb7), "item card did not accept a selected form")
		var item_name: Label = player.find_child("ItemName", true, false) as Label
		_check(item_name != null and item_name.text != "Select an item", "item card did not show the selected item")
		_check(player.call("open_container_inventory", target.file, target.object), "full container menu did not open")
		_check(player.call("is_menu_open"), "full container menu handoff did not open modal UI")
		player.call("close_menu")
	var one_state := await _open(player, take_one)
	var one_before := int(one_state.get("total", 0))
	_check(player.call("quick_loot_take", false) > 0, "take-one moved no item")
	await _frames()
	_check(int(player.call("quick_loot_snapshot").get("total", 0)) < one_before,
		"take-one did not refresh the shared inventory")
	if not take_all.is_empty():
		var all_state := await _open(player, take_all)
		var all_before := int(all_state.get("total", 0))
		_check(player.call("quick_loot_take", true) == all_before, "take-all did not move every displayed item")
		await _frames()
		var emptied: Dictionary = player.call("quick_loot_snapshot")
		_check(int(emptied.get("count", -1)) == 0, "take-all did not show an empty container")
		var view: Label = player.find_child("QuickLootState", true, false) as Label
		_check(view != null and view.text == "Empty", "empty state was not rendered")
	else:
		_check(false, "no container held multiple items for take-all")
	player.call("quick_loot_clear_forced_container")
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ui_26_quick_loot_scene")
	quit(0 if failures == 0 else 1)
