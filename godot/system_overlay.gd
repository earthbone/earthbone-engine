extends CanvasLayer

## PRF-12 presentation of Rust's fixed system slots. Monitor callbacks and
## units follow Godot's Performance documentation. Sampling reads completed
## frames and never uses Performance.TIME_PROCESS as a frame denominator.

var player: Node3D
var panel: PanelContainer
var label: Label
var elapsed := 0.0
var entities := false


func _ready() -> void:
	layer = 100
	panel = PanelContainer.new()
	panel.position = Vector2(8, 8)
	panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var background := StyleBoxFlat.new()
	background.bg_color = Color(0.02, 0.02, 0.02, 0.97)
	background.content_margin_left = 6.0
	background.content_margin_right = 6.0
	background.content_margin_top = 4.0
	background.content_margin_bottom = 4.0
	panel.add_theme_stylebox_override("panel", background)
	label = Label.new()
	label.add_theme_font_size_override("font_size", 12)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	panel.add_child(label)
	add_child(panel)
	panel.hide()
	refresh()
	if is_instance_valid(player):
		entities = player.call("entity_profiling_enabled")
		panel.visible = entities
		player.tree_exiting.connect(_write_on_player_exit)


func _input(event: InputEvent) -> void:
	if event is InputEventKey and event.keycode == KEY_F4 and event.pressed and not event.echo:
		entities = not entities
		player.call("set_entity_profiling", entities)
		panel.visible = true
		refresh()
		if not entities:
			player.call("write_entity_profile", ProjectSettings.globalize_path("user://"))
		get_viewport().set_input_as_handled()
	if event is InputEventKey and event.keycode == KEY_F3 and event.pressed and not event.echo:
		panel.visible = not panel.visible
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
	label.add_theme_font_size_override("font_size", 9 if entities and get_viewport().get_visible_rect().size.y < 400.0 else 12)
	if entities:
		label.text = player.call("entity_profile_overlay")
		return
	var report: Dictionary = player.call("system_frame_breakdown")
	var text := "F3: Engine systems, exclusive scope ms and % of wall frame\n"
	text += "System: last ms / frame % / rolling p50 / p99 / worst ms\n"
	for system in report:
		var row: Dictionary = report[system]
		text += "%s: %.3f / %.1f%% / %.3f / %.3f / %.3f\n" % [system, row.ms, row.percent, row.p50_ms, row.p99_ms, row.worst_ms]
		if system == "ai_movement" and row.has("phases"):
			var phases: Dictionary = row.phases
			text += "  Last simulation tick:\n"
			for phase in ["scheduling", "actor_driver", "movement", "procedure_tree", "path_following_dispatch"]:
				text += "  %s (inclusive): %.3f ms\n" % [phase, float(phases.get(phase + "_us", 0.0)) / 1000.0]
			for kind in ["navigation", "movement"]:
				text += "  %s workers: %.3f ms busy / %.3f ms caller wait / %d jobs\n" % [kind, float(phases.get(kind + "_worker_busy_us", 0.0)) / 1000.0, float(phases.get(kind + "_worker_wait_us", 0.0)) / 1000.0, int(phases.get(kind + "_worker_jobs", 0))]
			text += "  process high/middle high/middle low/low: %d/%d/%d/%d\n" % [int(phases.get("high_actors", 0)), int(phases.get("middle_high_actors", 0)), int(phases.get("middle_low_actors", 0)), int(phases.get("low_actors", 0))]
	label.text = text


func write_summary() -> Error:
	if not is_instance_valid(player):
		return ERR_UNAVAILABLE
	if not player.call("write_entity_profile", ProjectSettings.globalize_path("user://")):
		return ERR_CANT_CREATE
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
