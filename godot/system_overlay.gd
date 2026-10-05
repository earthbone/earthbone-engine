extends CanvasLayer

## PRF-12 presentation of Rust's fixed system slots. Monitor callbacks and
## units follow Godot's Performance documentation. Sampling reads completed
## frames and never uses Performance.TIME_PROCESS as a frame denominator.

var player: Node3D
var panel: PanelContainer
var label: Label
var elapsed := 0.0
var plugin_view := false


func _ready() -> void:
	layer = 100
	panel = PanelContainer.new()
	panel.position = Vector2(8, 8)
	panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label = Label.new()
	label.add_theme_font_size_override("font_size", 12)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size = Vector2(minf(get_viewport().get_visible_rect().size.x - 16.0, 620.0), minf(get_viewport().get_visible_rect().size.y - 16.0, 500.0))
	panel.add_child(scroll)
	scroll.add_child(label)
	add_child(panel)
	panel.hide()
	refresh()
	if is_instance_valid(player):
		player.tree_exiting.connect(_write_on_player_exit)


func _input(event: InputEvent) -> void:
	if event is InputEventKey and event.keycode == KEY_F3 and event.pressed and not event.echo:
		panel.visible = not panel.visible
		refresh()
		get_viewport().set_input_as_handled()
	elif event is InputEventKey and event.keycode == KEY_F4 and event.pressed and not event.echo:
		plugin_view = not plugin_view
		panel.show()
		refresh()
		get_viewport().set_input_as_handled()


func _process(delta: float) -> void:
	elapsed += delta
	if panel.visible and elapsed >= 0.25:
		elapsed = 0.0
		refresh()


func refresh() -> void:
	if not is_instance_valid(player):
		return
	var report: Dictionary = player.call("system_frame_breakdown")
	var text := "F3: Engine systems, exclusive scope ms and % of wall frame\n"
	text += "System: last ms / frame % / rolling p50 / p99 / worst ms\n"
	for system in report:
		var row: Dictionary = report[system]
		text += "%s: %.3f / %.1f%% / %.3f / %.3f / %.3f\n" % [system, row.ms, row.percent, row.p50_ms, row.p99_ms, row.worst_ms]
	var health: Dictionary = player.call("papyrus_health")
	var health_display := "Papyrus health, F4: %s\n" % ("per plugin" if plugin_view else "per script")
	health_display += health_text("Total", health.total)
	if plugin_view:
		for plugin in health.plugins:
			health_display += "%s\n" % plugin
			for script in health.plugins[plugin]:
				health_display += health_text(script, health.plugins[plugin][script])
	else:
		for script in health.scripts:
			health_display += health_text(script, health.scripts[script])
	label.text = health_display + "\n" + text if plugin_view else text + "\n" + health_display


func health_text(name: String, health: Dictionary) -> String:
	var text := "%s: backlog %d, oldest %.3fs, suspended %d\n" % [name, health.backlog, health.oldest_wait_seconds, health.suspended_stacks]
	text += "  Updates repeat/single %d/%d, game time %d/%d\n" % [health.repeating_updates, health.single_updates, health.repeating_game_time_updates, health.single_game_time_updates]
	text += "  Waiting %.3f stack s, running %.3f CPU ms\n" % [health.waiting_seconds, health.running_ms]
	for event in health.events:
		text += "  %s: queued %d, peak %d/s\n" % [event, health.events[event].queued, health.events[event].peak_per_second]
	return text


func write_summary() -> Error:
	if not is_instance_valid(player):
		return ERR_UNAVAILABLE
	var summary: String = player.call("system_summary")
	print(summary)
	var file := FileAccess.open("user://soe-systems.txt", FileAccess.WRITE)
	if file == null:
		return FileAccess.get_open_error()
	file.store_string(summary)
	return file.get_error()


func _write_on_player_exit() -> void:
	var error := write_summary()
	if error != OK:
		printerr("Could not write system summary: ", error_string(error))
