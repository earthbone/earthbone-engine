extends CanvasLayer

## PRF-12 presentation of Rust's fixed system slots. Monitor callbacks and
## units follow Godot's Performance documentation. Sampling reads completed
## frames and never uses Performance.TIME_PROCESS as a frame denominator.
const TransitionMetrics := preload("res://transition_metrics.gd")

var player: Node3D
var panel: PanelContainer
var label: Label
var elapsed := 0.0
var transition_metrics := TransitionMetrics.new()


func _ready() -> void:
	layer = 100
	panel = PanelContainer.new()
	panel.position = Vector2(8, 8)
	panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label = Label.new()
	label.add_theme_font_size_override("font_size", 12)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	panel.add_child(label)
	add_child(panel)
	panel.hide()
	refresh()
	if is_instance_valid(player):
		player.tree_exiting.connect(_write_on_player_exit)
		player.transition_requested.connect(_transition_requested)


func _input(event: InputEvent) -> void:
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
	var report: Dictionary = player.call("system_frame_breakdown")
	var text := "F3: Engine systems, exclusive scope ms and % of wall frame\n"
	text += "System: last ms / frame % / rolling p50 / p99 / worst ms\n"
	for system in report:
		var row: Dictionary = report[system]
		text += "%s: %.3f / %.1f%% / %.3f / %.3f / %.3f\n" % [system, row.ms, row.percent, row.p50_ms, row.p99_ms, row.worst_ms]
	label.text = text


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


func write_transition_summary() -> Error:
	var summary: String = transition_metrics.summary_text()
	print(summary)
	var file := FileAccess.open("user://soe-transitions.txt", FileAccess.WRITE)
	if file == null:
		return FileAccess.get_open_error()
	file.store_string(summary)
	return file.get_error()


func _transition_requested(kind: String, name: String) -> void:
	transition_metrics.request(kind, name)


func _write_on_player_exit() -> void:
	var error := write_summary()
	if error != OK:
		printerr("Could not write system summary: ", error_string(error))
	error = write_transition_summary()
	if error != OK:
		printerr("Could not write transition summary: ", error_string(error))
