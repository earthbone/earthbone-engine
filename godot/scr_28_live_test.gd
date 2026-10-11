extends SceneTree

## Live PlayerNode checks for SCR-28 camera and fade presentation.
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
		printerr("FAIL: scr_28_live_test requires SKYRIM_DATA and the extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "vanilla data load failed")
	_check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0,
		"Bannered Mare render load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"Bannered Mare collision load failed")
	_check(player.call("start_gameplay"), "runtime start failed")
	var overlay := player.get_node_or_null("DoorFadeLayer/DoorFade") as ColorRect
	_check(overlay != null, "fade overlay is missing")
	_check(player.call("debug_invoke_scr_28_native", "ForceFirstPerson", false, false, 0.0, 0.0),
		"ForceFirstPerson native did not complete")
	_check(not player.call("is_third_person"), "ForceFirstPerson did not change live camera mode")
	_check(player.call("debug_invoke_scr_28_native", "ForceThirdPerson", false, false, 0.0, 0.0),
		"ForceThirdPerson native did not complete")
	_check(player.call("is_third_person"), "ForceThirdPerson did not change live camera mode")
	_check(player.call("debug_invoke_scr_28_native", "FadeOutGame", true, false, 0.0, 1.0),
		"white FadeOutGame native did not complete")
	player.call("advance", 0.5)
	if overlay != null:
		_check(overlay.visible, "white game fade did not show the overlay")
		_check(overlay.color.is_equal_approx(Color(1.0, 1.0, 1.0, 0.5)),
			"white game fade did not render its requested color and opacity: " + str(overlay.color))
	_check(player.call("debug_invoke_scr_28_native", "FadeOutGame", true, true, 0.0, 1.0),
		"black FadeOutGame native did not complete")
	player.call("advance", 0.5)
	if overlay != null:
		_check(overlay.color.is_equal_approx(Color(0.0, 0.0, 0.0, 0.5)),
			"black game fade did not render its requested color and opacity: " + str(overlay.color))
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " scr_28 live host")
	player.free()
	streamer.free()
	quit(0 if failures == 0 else 1)
