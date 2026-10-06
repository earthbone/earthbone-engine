extends SceneTree

## PRF-19 live adapter and both overlay views, with installed scripts when set.
const Overlay := preload("res://system_overlay.gd")
const FIELDS := ["backlog", "oldest_wait_seconds", "repeating_updates", "single_updates", "repeating_game_time_updates", "single_game_time_updates", "waiting_seconds", "running_ms", "suspended_stacks", "events"]
var failures := 0


func _initialize() -> void:
	call_deferred("prf_19_host_health_and_plugin_overlay")


func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-19 FAIL: ", message)


func press(key: Key) -> void:
	var event := InputEventKey.new()
	event.keycode = key
	event.pressed = true
	Input.parse_input_event(event)
	Input.flush_buffered_events()


func prf_19_host_health_and_plugin_overlay() -> void:
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	var report: Dictionary = player.call("papyrus_health")
	for field in FIELDS:
		check(report.total.has(field), "missing field " + field)
	check(report.total.backlog == 0 and report.total.suspended_stacks == 0, "idle counts")
	var data := OS.get_environment("SKYRIM_DATA")
	var streamer: Node3D
	if not data.is_empty():
		streamer = ClassDB.instantiate("CellStreamerNode")
		get_root().add_child(streamer)
		check(streamer.call("load_data", data), "data load failed")
		check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0, "scene load failed")
		check(player.call("use_streamer_data", streamer), "shared data source failed")
		check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "cell load failed")
		check(player.call("start_gameplay"), "runtime start failed")
		player.call("advance", 1.0 / 60.0)
		report = player.call("papyrus_health")
		check(not report.scripts.is_empty(), "live VM has no script rows")
		check(report.plugins.has("Skyrim.esm"), "live VM has no vanilla plugin rows")
		for script in report.scripts:
			for field in FIELDS:
				check(report.scripts[script].has(field), "script row missing " + field)
	var overlay := Overlay.new()
	overlay.player = player
	get_root().add_child(overlay)
	press(KEY_F3)
	check(overlay.panel.visible, "F3 did not show overlay")
	check(overlay.label.text.contains("Papyrus health"), "health is absent")
	check(overlay.label.text.contains("backlog") and overlay.label.text.contains("Waiting") and overlay.label.text.contains("Updates repeat/single"), "health fields are absent")
	for script in report.scripts:
		check(overlay.label.text.contains(script), "script row is absent")
	press(KEY_F4)
	check(overlay.plugin_view, "F4 did not select plugin view")
	check(not overlay.entities, "F4 unexpectedly enabled entity profiling")
	for plugin in report.plugins:
		check(overlay.label.text.contains(plugin), "plugin row is absent")
	var capture := OS.get_environment("SOE_PRF_19_CAPTURE")
	if DisplayServer.get_name() != "headless" and not capture.is_empty():
		await process_frame
		await RenderingServer.frame_post_draw
		check(get_root().get_texture().get_image().save_png(capture) == OK, "capture failed")
	player.free()
	overlay.free()
	if is_instance_valid(streamer):
		streamer.free()
	print("prf_19_host_health_and_plugin_overlay: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
