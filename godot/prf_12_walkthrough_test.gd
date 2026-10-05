extends "res://walkthrough.gd"

## Installed-data acceptance of the actual walkthrough exit path.
var prf_12_failures := 0


func _process(delta: float) -> bool:
	if not loaded:
		return super._process(delta)
	frames += 1
	if frames == 4:
		var event := InputEventKey.new()
		event.keycode = KEY_F3
		event.pressed = true
		Input.parse_input_event(event)
		Input.flush_buffered_events()
	if frames == 12:
		var report: Dictionary = player.call("system_frame_breakdown")
		for system in ["papyrus_vm", "ai_planning", "magic", "quests_story_manager", "dialogue"]:
			if not Performance.has_custom_monitor("SOE/" + system + "_ms") or report[system].worst_ms <= 0.0:
				prf_12_failures += 1
				printerr("PRF-12 walkthrough missing runtime time: ", system)
		if not system_overlay.panel.visible:
			prf_12_failures += 1
			printerr("PRF-12 walkthrough F3 overlay is hidden")
		print("prf_12_walkthrough_runtime_and_exit_summary: ", "PASS" if prf_12_failures == 0 else "FAIL")
		quit(0 if prf_12_failures == 0 else 1)
	return false
