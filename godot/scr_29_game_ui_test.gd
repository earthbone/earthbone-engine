extends SceneTree

## SCR-29 visible host check for the live race menu and HUD presentation nodes.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: scr_29_game_ui_test requires SKYRIM_DATA and the extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "game data did not load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source was rejected")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"player collision did not load")
	_check(player.call("start_gameplay"), "game runtime did not start")
	var cart_badge := player.get_node_or_null("HudLayer/Hud/CartMode") as Label
	var blood_overlay := player.get_node_or_null("HudLayer/Hud/ScreenBlood") as ColorRect
	_check(cart_badge != null, "cart-mode HUD presenter is missing")
	_check(blood_overlay != null, "screen blood overlay presenter is missing")
	if cart_badge != null:
		_check(not cart_badge.visible, "cart mode should start hidden")
	_check(player.call("show_race_menu"), "race menu request did not open")
	var menu := player.get_node_or_null("MenuLayer/MenuPanel") as Control
	_check(menu != null and menu.visible, "race menu is not visible in the host UI")
	if menu != null and menu.visible:
		_check(menu.get_node_or_null("Layout/Content") != null,
			"race menu opened without visible content")
	player.call("close_menu")
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " SCR-29 visible UI")
	quit(0 if failures == 0 else 1)
