extends CanvasLayer

## PRF-12 presentation of Rust's fixed system slots. Monitor callbacks and
## units follow Godot's Performance documentation. Sampling reads completed
## frames and never uses Performance.TIME_PROCESS as a frame denominator.

var player: Node3D
var panel: PanelContainer
var label: Label
var elapsed := 0.0
var frame_samples: Array[float] = []
const FRAME_SAMPLE_LIMIT := 240


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


func _input(event: InputEvent) -> void:
	if event is InputEventKey and event.keycode == KEY_F3 and event.pressed and not event.echo:
		panel.visible = not panel.visible
		refresh()
		get_viewport().set_input_as_handled()


func _process(delta: float) -> void:
	frame_samples.append(delta * 1000.0)
	if frame_samples.size() > FRAME_SAMPLE_LIMIT:
		frame_samples.pop_front()
	elapsed += delta
	if panel.visible and elapsed >= 0.25:
		elapsed = 0.0
		refresh()


func refresh() -> void:
	if not is_instance_valid(player):
		return
	var report: Dictionary = player.call("system_frame_breakdown")
	var runtime: Dictionary = player.call("performance_metrics")
	var text := "F3: Engine systems, exclusive scope ms and % of wall frame\n"
	text += "System: last ms / frame % / rolling p50 / p99 / worst ms\n"
	for system in report:
		var row: Dictionary = report[system]
		text += "%s: %.3f / %.1f%% / %.3f / %.3f / %.3f\n" % [system, row.ms, row.percent, row.p50_ms, row.p99_ms, row.worst_ms]
	text += "PRF-24 runtime metrics (cumulative unless gauge):\n"
	for name: String in runtime:
		text += "%s: %d\n" % [name, runtime[name]]
	var pacing := frame_pacing()
	text += "frame_ms stddev %.3f / p50 %.3f / p99 %.3f / worst %.3f\n" % [pacing.stddev_ms, pacing.p50_ms, pacing.p99_ms, pacing.worst_ms]
	label.text = text


func frame_pacing() -> Dictionary:
	var sorted := frame_samples.duplicate()
	sorted.sort()
	if sorted.is_empty():
		return {"stddev_ms": 0.0, "p50_ms": 0.0, "p99_ms": 0.0, "worst_ms": 0.0}
	var mean := 0.0
	for sample in sorted:
		mean += sample
	mean /= sorted.size()
	var variance := 0.0
	for sample in sorted:
		variance += pow(sample - mean, 2.0)
	variance /= sorted.size()
	return {
		"stddev_ms": sqrt(variance),
		"p50_ms": sorted[int((sorted.size() - 1) * 0.50)],
		"p99_ms": sorted[int((sorted.size() - 1) * 0.99)],
		"worst_ms": sorted[-1],
	}


func write_summary() -> Error:
	if not is_instance_valid(player):
		return ERR_UNAVAILABLE
	var summary: String = player.call("system_summary")
	summary += "\nPRF-24 runtime metrics: " + JSON.stringify(player.call("performance_metrics"))
	summary += "\nPRF-24 frame pacing: " + JSON.stringify(frame_pacing()) + "\n"
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
