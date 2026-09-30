extends SceneTree

## UI-07 live acceptance check: open the race menu, change race and sex through
## the connected Godot buttons, and confirm the selection against game records.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _button(rows: VBoxContainer, text: String) -> Button:
	for child in rows.get_children():
		if child is Button and child.text == text:
			return child as Button
	return null

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: ui_07_race_menu_test requires SKYRIM_DATA and the extension")
		quit(1)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "player did not share game records")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"player collision did not load")
	_check(player.call("start_gameplay"), "game runtime did not start")
	_check(player.call("show_race_menu"), "race menu did not open")
	_check(player.call("is_menu_open"), "race menu presenter left the modal closed")

	var panel: Control = player.get_node("MenuLayer/MenuPanel")
	var content: VBoxContainer = panel.get_node("Layout/Content")
	var rows: VBoxContainer = content.get_child(0)
	var current_sex := ""
	var alternate_race: Button
	for child in rows.get_children():
		if child is Button:
			var button := child as Button
			if button.text == "Male *":
				current_sex = "Male"
			elif button.text == "Female *":
				current_sex = "Female"
			if button.text != "Male" and button.text != "Male *" \
				and button.text != "Female" and button.text != "Female *" \
				and button.text != "Confirm" and not button.text.ends_with(" *") \
				and alternate_race == null:
				alternate_race = button

	_check(current_sex != "", "menu did not mark the player's current sex")
	_check(alternate_race != null, "menu did not offer another playable race record")
	_check(_button(rows, "Male") != null or _button(rows, "Male *") != null,
		"male sex choice was not rendered")
	_check(_button(rows, "Female") != null or _button(rows, "Female *") != null,
		"female sex choice was not rendered")
	_check(_button(rows, "Confirm") != null, "confirmation button was not rendered")
	if alternate_race != null:
		var selected_name := alternate_race.text
		alternate_race.pressed.emit()
		await process_frame
		rows = content.get_child(0)
		var changed_race := false
		for child in rows.get_children():
			if child is Button and child.text == selected_name + " *":
				changed_race = true
		_check(changed_race, "race button callback did not select its record")
	if current_sex == "Male":
		var female := _button(rows, "Female")
		if female != null:
			female.pressed.emit()
	elif current_sex == "Female":
		var male := _button(rows, "Male")
		if male != null:
			male.pressed.emit()
	await process_frame
	rows = content.get_child(0)
	var selected_sex := "Female *" if current_sex == "Male" else "Male *"
	var changed_sex := false
	for child in rows.get_children():
		if child is Button and child.text == selected_sex:
			changed_sex = true
	_check(changed_sex, "sex button callback did not change the selected sex")
	var confirm_button := _button(rows, "Confirm")
	if confirm_button != null:
		confirm_button.pressed.emit()
	await process_frame
	_check(not player.call("is_menu_open"), "confirmation did not close the race menu")
	for i in range(2):
		player.call("advance", 1.0 / 60.0)
	_check(not player.call("is_menu_open"), "confirmed menu reopened on a gameplay tick")

	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ui_07 race menu")
	quit(0 if failures == 0 else 1)
