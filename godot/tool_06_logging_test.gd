extends SceneTree

var failures := 0

func check(condition: bool, label: String) -> void:
	if not condition:
		failures += 1
		printerr("TOOL06 failed: ", label)

func _initialize() -> void:
	call_deferred("run_check")

func run_check() -> void:
	var bridge: Node = ClassDB.instantiate("SoeLogBridge")
	root.add_child(bridge)
	check(bridge.call("log_filter") == "warn", "SOE_LOG startup filter")
	check(bridge.call("console", "log error") == "error", "console reload")
	bridge.call("tool_06_emit")
	bridge.call("flush_logs")
	check(bridge.call("set_log_filter", "trace"), "reload all levels")
	check(not bridge.call("set_log_filter", "game=invalid"), "invalid filter rejected")
	check(bridge.call("log_filter") == "trace", "invalid filter preserves previous")
	var begin: int = bridge.call("log_time_ms")
	for _index in range(20):
		bridge.call("tool_06_emit")
	var end: int = bridge.call("log_time_ms")
	var lines: PackedStringArray = bridge.call("log_window", begin, end)
	check(lines.size() >= 3, "release error, warning and info retained")
	check("\n".join(lines).contains("count=20"), "repeated messages collapsed")
	check(not "\n".join(lines).contains("tool_06 debug"), "release debug compiled out")
	check(not "\n".join(lines).contains("tool_06 trace"), "release trace compiled out")
	bridge.call("flush_logs")
	check(bridge.call("dump_log_window", "user://logs/tool_06_hitch.log", begin, end), "hitch window file")
	var hitch := FileAccess.get_file_as_string("user://logs/tool_06_hitch.log")
	check(hitch.contains("tool_06 weapon not equipped") and hitch.contains("count=20"), "hitch carries its log window")
	var file := FileAccess.get_file_as_string("user://logs/engine.log")
	check(file.contains("tool_06 weapon not equipped") and file.contains("tool_06 warning") and file.contains("tool_06 info"), "all enabled levels reach rotating file")
	check(file.contains("form_id=") and file.contains("plugin=") and file.contains("script=") and file.contains("actor=") and file.contains("system="), "structured context reaches file")
	var diagnostics: Node = root.get_node_or_null("SoeDiagnostics")
	if diagnostics == null:
		diagnostics = load("res://diagnostics.gd").new()
		root.add_child(diagnostics)
	var event := InputEventKey.new()
	event.keycode = KEY_F3
	event.pressed = true
	diagnostics._input(event)
	check(diagnostics.panel.visible, "F3 shows verbosity controls")
	diagnostics._apply_filter("error,papyrus=warn")
	check(bridge.call("log_filter") == "error,papyrus=warn", "F3 reloads subscriber")
	diagnostics._input(event)
	check(not diagnostics.panel.visible, "F3 closes controls")
	event.keycode = KEY_QUOTELEFT
	diagnostics._input(event)
	check(diagnostics.panel.visible, "console opens")
	diagnostics._console("log warn")
	check(bridge.call("log_filter") == "warn", "console panel reloads subscriber")
	diagnostics._input(event)
	await process_frame
	print("tool_06_godot_routing: failures=", failures)
	bridge.queue_free()
	await process_frame
	quit(0 if failures == 0 else 1)
