# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree
## Installed merchant dialogue and menu clicks over a recorded Game fixture.

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> bool:
	if not ok:
		failures += 1
		printerr("FAIL ui_31: ", message)
	return ok

func _presenter(player: Node) -> Control:
	var content: Control = player.call("menu_content")
	return content.get_node_or_null("ItemMenu") as Control if content != null else null

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var cell := OS.get_environment("SOE_UI_31_CELL")
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	if not _check(streamer.call("load_data", data), "load installed data"):
		quit(1)
		return
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "share installed data")
	_check(player.call("enter_cell", cell, 0.0, 0.0, 500.0) > 0, "enter merchant cell")
	_check(player.call("start_gameplay"), "start authored scripts")
	if not _check(player.call("replay_player_session", OS.get_environment("SOE_UI_31_SESSION")), "restore recorded fixture"):
		quit(1)
		return
	var merchant_file := int(OS.get_environment("SOE_UI_31_MERCHANT_FILE"))
	var merchant_object := int(OS.get_environment("SOE_UI_31_MERCHANT_OBJECT"))
	_check(player.call("open_dialogue_menu", merchant_file, merchant_object), "talk to Belethor")
	var state: Dictionary = player.call("dialogue_state")
	var topic: Dictionary = {}
	for entry: Dictionary in state.get("topics", []):
		if "sale" in str(entry.label).to_lower():
			topic = entry
			break
	if not _check(not topic.is_empty(), "authored barter topic offered"):
		print("ui_31 dialogue: ", state)
		quit(1)
		return
	var content: Control = player.call("menu_content")
	var button: Button
	for node: Node in content.find_children("*", "Button", true, false):
		if (node as Button).text == topic.label:
			button = node as Button
			break
	if not _check(button != null, "barter topic is a dialogue button"):
		quit(1)
		return
	button.emit_signal("pressed")
	for response in range(16):
		player.call("advance", 0.0)
		if _presenter(player) != null:
			break
		_check(player.call("advance_dialogue_menu"), "complete authored response")
	var view := _presenter(player)
	if not _check(view != null and view.get("mode") == "barter", "fragment opens barter during dialogue"):
		quit(1)
		return
	var stock: Array = view.get("source_rows").filter(func(row): return int(row.side) == 0)
	if not _check(not stock.is_empty(), "merchant chest has priced stock"):
		quit(1)
		return
	var item: Dictionary = stock[0]
	var initial_count := int(player.call("play_path_item_count", int(item.file), int(item.object)))
	var gold := int(view.get("player_gold"))
	var merchant_gold := int(view.get("merchant_gold"))
	_check(int(item.value) > 0 and int(item.value) <= gold, "affordable buy quote")
	view.call("_select", item)
	var confirm := view.get_node("Body/ItemCard/Confirm") as Button
	confirm.emit_signal("pressed")
	_check(int(player.call("play_path_item_count", int(item.file), int(item.object))) == initial_count + 1, "buy moves item to player")
	_check(int(view.get("player_gold")) == gold - int(item.value), "buy takes quoted gold")
	_check(int(view.get("merchant_gold")) == merchant_gold + int(item.value), "buy credits merchant chest")
	var remaining: Array = view.get("source_rows").filter(func(row): return int(row.side) == 0 and row.id == item.id)
	_check((remaining.is_empty() and int(item.count) == 1) or (not remaining.is_empty() and int(remaining[0].count) == int(item.count) - 1), "buy reduces chest stock")
	view.call("select_side", 1)
	var sales: Array = view.get("source_rows").filter(func(row): return int(row.side) == 1 and row.id == item.id)
	if _check(not sales.is_empty(), "purchased item appears on sell side"):
		var sale: Dictionary = sales[0]
		view.call("_select", sale)
		(view.get_node("Body/ItemCard/Confirm") as Button).emit_signal("pressed")
		_check(int(player.call("play_path_item_count", int(item.file), int(item.object))) == initial_count, "sell returns item to chest")
		_check(int(view.get("player_gold")) == gold - int(item.value) + int(sale.value), "sell credits quoted gold")
		_check(int(view.get("merchant_gold")) == merchant_gold + int(item.value) - int(sale.value), "sell debits merchant chest")
	_check(player.call("close_menu"), "close barter")
	_check(player.call("is_menu_open"), "dialogue remains modal")
	var title := player.get_node("MenuLayer/MenuPanel/Layout/Title") as Label
	_check(title.text == "Dialogue", "close returns to dialogue")
	state = player.call("dialogue_state")
	_check(int(state.get("speaker_object", -1)) == merchant_object, "same speaker resumes")
	_check(not state.get("topics", []).is_empty(), "dialogue topics resume")
	_check(player.call("close_menu"), "close dialogue")
	_check(not player.call("is_menu_open"), "return to play")
	print("PASS ui_31" if failures == 0 else "FAIL ui_31 checks=" + str(failures))
	player.free()
	streamer.free()
	quit(0 if failures == 0 else 1)
