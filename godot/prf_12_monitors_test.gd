extends SceneTree

const Overlay := preload("res://system_overlay.gd")
const SYSTEMS := ["papyrus_vm", "ai_planning", "ai_movement", "combat", "magic", "quests_story_manager", "dialogue", "physics_sync", "cell_streaming", "audio", "save", "input", "host_render_submission", "other"]
const HOST_PHASES := ["reference_collision_sync", "reference_change_sync", "node_transforms", "mesh_material_uploads", "rendering_server_calls", "host_scene_other", "host_scene_cpu"]
var failures := 0


func _initialize() -> void:
	call_deferred("prf_12_godot_monitors_overlay_and_summary")


func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-12 FAIL: ", message)


func press_f3(echo: bool = false) -> void:
	var event := InputEventKey.new()
	event.keycode = KEY_F3
	event.pressed = true
	event.echo = echo
	Input.parse_input_event(event)
	Input.flush_buffered_events()


func prf_12_godot_monitors_overlay_and_summary() -> void:
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	var overlay := Overlay.new()
	overlay.player = player
	get_root().add_child(overlay)
	for system in SYSTEMS:
		check(Performance.has_custom_monitor("SOE/" + system + "_ms"), "missing " + system)
	var second: Node3D = ClassDB.instantiate("PlayerNode")
	second.set("capture_mouse", false)
	second.set("auto_step", false)
	get_root().add_child(second)
	check(Performance.get_custom_monitor_names().size() >= SYSTEMS.size() * 2, "second player monitor names collide")
	second.free()
	for _frame in 4:
		player.call("advance", 1.0 / 60.0)
		await process_frame
	var rows: Dictionary = player.call("system_frame_breakdown")
	check(rows.size() == SYSTEMS.size() + HOST_PHASES.size(), "incomplete breakdown")
	prf_34_host_scene_phases_are_exclusive(rows)
	var share := 0.0
	for system in SYSTEMS:
		var value: float = Performance.get_custom_monitor("SOE/" + system + "_ms")
		check(is_finite(value) and value >= 0.0 and is_equal_approx(value, rows[system].ms), "invalid monitor value " + system)
		check(rows[system].p99_ms >= rows[system].p50_ms and rows[system].worst_ms >= rows[system].p99_ms, "invalid percentiles " + system)
		share += rows[system].percent
	check(share <= 100.0001, "nested scopes double count time")
	check(rows.input.ms > 0.0 and rows.host_render_submission.ms > 0.0, "host paths not timed")
	press_f3()
	check(overlay.panel.visible, "F3 did not open overlay")
	press_f3(true)
	check(overlay.panel.visible, "key echo toggled overlay")
	for system in SYSTEMS + HOST_PHASES:
		check(overlay.label.text.contains(system), "overlay missing " + system)
	check(overlay.label.text.contains("%"), "overlay missing frame shares")
	if DisplayServer.get_name() != "headless":
		await process_frame
		await RenderingServer.frame_post_draw
		check(overlay.panel.position.y + overlay.panel.size.y <= get_root().size.y, "PRF-34 overlay rows are clipped")
		var capture := OS.get_environment("SOE_PRF_12_CAPTURE")
		if not capture.is_empty():
			check(get_root().get_texture().get_image().save_png(capture) == OK, "overlay capture failed")
	press_f3()
	check(not overlay.panel.visible, "F3 did not close overlay")
	check(overlay.write_summary() == OK, "summary write failed")
	var summary := FileAccess.get_file_as_string("user://soe-systems.txt")
	for system in SYSTEMS + HOST_PHASES:
		check(summary.contains(system), "exit summary missing " + system)
	check(DirAccess.remove_absolute(ProjectSettings.globalize_path("user://soe-systems.txt")) == OK, "could not remove prior summary")
	player.free()
	check(FileAccess.get_file_as_string("user://soe-systems.txt").contains("host_render_submission"), "player exit did not write summary")
	overlay.free()
	for system in SYSTEMS:
		check(not Performance.has_custom_monitor("SOE/" + system + "_ms"), "monitor survived player removal")
	Performance.add_custom_monitor("SOE/audio_ms", func() -> float: return 3.0)
	var third: Node3D = ClassDB.instantiate("PlayerNode")
	third.set("capture_mouse", false)
	third.set("auto_step", false)
	get_root().add_child(third)
	check(Performance.get_custom_monitor("SOE/audio_ms") == 3.0, "partially occupied category was overwritten")
	check(Performance.get_custom_monitor_names().size() >= SYSTEMS.size() + 1, "partial-category collision lost a monitor")
	third.free()
	check(Performance.has_custom_monitor("SOE/audio_ms"), "cleanup removed another owner's monitor")
	Performance.remove_custom_monitor("SOE/audio_ms")
	print("prf_12_godot_monitors_overlay_and_summary: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)


func prf_34_host_scene_phases_are_exclusive(rows: Dictionary) -> void:
	var own := 0.0
	for phase in HOST_PHASES:
		check(rows.has(phase), "PRF-34 missing host phase " + phase)
		if phase not in ["host_scene_cpu", "rendering_server_calls"]:
			own += rows[phase].ms
	check(is_equal_approx(own, rows.host_scene_cpu.ms), "PRF-34 scene CPU includes nested phases twice")
	check(own + rows.rendering_server_calls.ms <= rows.host_render_submission.ms + 0.0001, "PRF-34 phase time exceeds render slot")
	print("prf_34_host_scene_phases_are_exclusive: ", "PASS" if failures == 0 else "FAIL")
