extends SceneTree

const Overlay := preload("res://system_overlay.gd")
var failures := 0


func _initialize() -> void:
	call_deferred("prf_27_windowed_ai_breakdown_and_overlay")


func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-27 FAIL: ", message)


func prf_27_windowed_ai_breakdown_and_overlay() -> void:
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	player.call("set_entity_profiling", false)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	var data := OS.get_environment("SKYRIM_DATA")
	if not data.is_empty():
		var loaded: bool = streamer.call("load_data", data)
		loaded = loaded and player.call("use_streamer_data", streamer)
		loaded = loaded and player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0
		loaded = loaded and player.call("start_gameplay")
		check(loaded, "installed gameplay startup")
		if loaded:
			for frame in 120:
				player.call("advance", 1.0 / 60.0)
				if frame % 30 == 0:
					await process_frame
	var report: Dictionary = player.call("ai_breakdown")
	for procedure in ["Travel", "Sandbox", "Follow", "Escort", "Orbit", "UseItemAt", "UseMagic", "UseWeapon"]:
		check(report.procedures.has(procedure), "missing procedure " + procedure)
	for phase in ["package_selection", "schedule_evaluation", "combat_target", "combat_style_choice", "spell_choice", "weapon_choice", "greeting_evaluation", "force_greet_evaluation", "idle_chatter_evaluation", "sandbox_markers", "steering"]:
		check(report.phases.has(phase), "missing phase " + phase)
	for level in ["high", "middle_high", "middle_low", "low"]:
		check(report.levels.has(level), "missing process level " + level)
	check(not player.call("entity_profiling_enabled"), "breakdown enabled optional capture")
	if not data.is_empty():
		check(report.levels.high.actors >= 5, "installed actors not counted")
		check(not report.paths.is_empty(), "installed path requests not recorded")
		check(report.phases.has("actor_driver") and report.phases.has("navigation_resume"), "PRF-11 phases missing")
		for actor in report.paths:
			for sample in report.paths[actor].samples:
				check(sample.latency_us >= 0.0 and sample.nodes_expanded >= 0 and sample.length >= 0.0, "invalid path sample")
	var overlay := Overlay.new()
	overlay.player = player
	root.add_child(overlay)
	var event := InputEventKey.new()
	event.keycode = KEY_F5
	event.pressed = true
	Input.parse_input_event(event)
	Input.flush_buffered_events()
	check(overlay.panel.visible, "F5 did not show overlay")
	check(overlay.label.text.contains("STUCK (5 s):"), "stuck section missing")
	check(overlay.label.text.contains("Travel") and overlay.label.text.contains("middle_low"), "AI breakdown labels missing")
	if DisplayServer.get_name() != "headless":
		await process_frame
		await RenderingServer.frame_post_draw
		var bounds := overlay.panel.get_global_rect()
		check(bounds.end.x <= root.get_visible_rect().end.x and bounds.end.y <= root.get_visible_rect().end.y, "AI overlay exceeds viewport")
		var capture := OS.get_environment("SOE_PRF_27_CAPTURE")
		if not capture.is_empty():
			check(root.get_texture().get_image().save_png(capture) == OK, "overlay capture failed")
	print("prf_27_windowed_ai_breakdown_and_overlay: ", "PASS" if failures == 0 else "FAIL")
	overlay.free()
	player.free()
	streamer.free()
	quit(0 if failures == 0 else 1)
