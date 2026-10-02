extends SceneTree

## UI-06 live host check: start gameplay and inspect the rendered native HUD.
## Run with SKYRIM_DATA set and the release extension built.
var failures := 0
const CELL := "WhiterunBanneredMare"

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _region_has_ink(image: Image, control: Control) -> bool:
	var rect := control.get_global_rect()
	var left := maxi(0, floori(rect.position.x))
	var top := maxi(0, floori(rect.position.y))
	var right := mini(image.get_width(), ceili(rect.end.x))
	var bottom := mini(image.get_height(), ceili(rect.end.y))
	for y in range(top, bottom):
		for x in range(left, right):
			if image.get_pixel(x, y).get_luminance() > 0.2:
				return true
	return false

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: ui_06_hud_render_test requires SKYRIM_DATA and the extension")
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
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0,
		"player collision did not load")
	_check(player.call("start_gameplay"), "game runtime did not start")

	await process_frame
	await process_frame
	await process_frame
	var hud: Control = player.get_node("HudLayer/Hud")
	var meters: VBoxContainer = hud.get_node("StatusMeters")
	var frame := get_root().get_texture().get_image()
	_check(hud.is_visible_in_tree(), "HUD CanvasLayer controls were not visible")
	_check(meters.get_child_count() == 6, "HUD did not render three label and bar pairs")
	var expected := ["Health", "Magicka", "Stamina"]
	for index in range(expected.size()):
		var label := meters.get_child(index * 2) as Label
		var bar := meters.get_child(index * 2 + 1) as ProgressBar
		_check(label != null and label.text.begins_with(expected[index]),
			"missing rendered %s value" % expected[index])
		_check(bar != null and bar.is_visible_in_tree(),
			"%s meter bar was not rendered" % expected[index])
		_check(bar != null and bar.value >= 0.0 and bar.value <= 100.0,
			"%s meter fill was outside its bounded range" % expected[index])
		_check(label != null and _region_has_ink(frame, label),
			"%s label produced no viewport pixels" % expected[index])
		_check(bar != null and _region_has_ink(frame, bar),
			"%s meter produced no viewport pixels" % expected[index])
	var compass := hud.get_node("Compass") as Label
	_check(compass != null and compass.is_visible_in_tree(), "compass was not rendered")
	_check(compass != null and compass.text.contains("[N]"),
		"north heading was not rendered under the compass center")
	_check(compass != null and _region_has_ink(frame, compass),
		"compass produced no viewport pixels")

	var initial_labels: Array[String] = []
	var initial_fills: Array[float] = []
	for index in range(expected.size()):
		var label := meters.get_child(index * 2) as Label
		var bar := meters.get_child(index * 2 + 1) as ProgressBar
		initial_labels.append(label.text)
		initial_fills.append(bar.value)
	for value_name in ["health", "magicka", "stamina"]:
		_check(player.call("damage_player_actor_value", value_name, 20.0),
			"could not change live %s actor value" % value_name)
	await process_frame
	await process_frame
	var updated_frame := get_root().get_texture().get_image()
	for index in range(expected.size()):
		var label := meters.get_child(index * 2) as Label
		var bar := meters.get_child(index * 2 + 1) as ProgressBar
		_check(label.text != initial_labels[index],
			"%s HUD label did not update after its actor value changed" % expected[index])
		_check(bar.value < initial_fills[index],
			"%s HUD fill did not update after its actor value changed" % expected[index])
		_check(_region_has_ink(updated_frame, label),
			"updated %s label produced no viewport pixels" % expected[index])
		_check(_region_has_ink(updated_frame, bar),
			"updated %s meter produced no viewport pixels" % expected[index])

	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ui_06 HUD render")
	quit(0 if failures == 0 else 1)
