extends CanvasLayer

## PRF-12 presentation of Rust's fixed system slots. Monitor callbacks and
## units follow Godot's Performance documentation. Sampling reads completed
## frames and never uses Performance.TIME_PROCESS as a frame denominator.
const TransitionMetrics := preload("res://transition_metrics.gd")

var player: Node3D
var panel: PanelContainer
var label: Label
var elapsed := 0.0
var entities := false
var ai := false
var frame_samples: Array[float] = []
const FRAME_SAMPLE_LIMIT := 240
var transition_metrics := TransitionMetrics.new()
var last_simulation := {}


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
		player.transition_requested.connect(_transition_requested)


func _input(event: InputEvent) -> void:
	if event is InputEventKey and event.keycode == KEY_F5 and event.pressed and not event.echo:
		ai = not ai
		panel.visible = true
		refresh()
		get_viewport().set_input_as_handled()
	if event is InputEventKey and event.keycode == KEY_F4 and event.pressed and not event.echo:
		ai = false
		entities = not entities
		player.call("set_entity_profiling", entities)
		panel.visible = true
		refresh()
		if not entities:
			player.call("write_entity_profile", ProjectSettings.globalize_path("user://"))
		get_viewport().set_input_as_handled()
	if event is InputEventKey and event.keycode == KEY_F3 and event.pressed and not event.echo:
		ai = false
		panel.visible = not panel.visible
		refresh()
		get_viewport().set_input_as_handled()


func _process(delta: float) -> void:
	frame_samples.append(delta * 1000.0)
	if frame_samples.size() > FRAME_SAMPLE_LIMIT:
		frame_samples.pop_front()
	elapsed += delta
	if elapsed >= 0.25:
		elapsed = 0.0
		_capture_simulation()
		if panel.visible:
			refresh()


func refresh() -> void:
	if not is_instance_valid(player):
		return
	label.add_theme_font_size_override("font_size", 9 if entities and get_viewport().get_visible_rect().size.y < 400.0 else 12)
	label.add_theme_constant_override("line_spacing", 3)
	if ai:
		label.add_theme_font_size_override("font_size", 9)
		label.add_theme_constant_override("line_spacing", -3)
		label.text = ai_text(player.call("ai_breakdown"))
		return
	if entities:
		label.text = player.call("entity_profile_overlay")
		return
	var report: Dictionary = player.call("system_frame_breakdown")
	var runtime: Dictionary = player.call("performance_metrics")
	var text := "F3: Engine systems, exclusive scope ms and % of wall frame\n"
	var total: Dictionary = report.main_thread_engine
	text += "Main engine: %.3f ms / p50 %.3f / p99 %.3f / worst %.3f\n" % [total.ms, total.p50_ms, total.p99_ms, total.worst_ms]
	var simulation := _capture_simulation()
	if simulation.ready:
		text += "Simulation: %.3f ms / workers %.3f ms / lag %.3f s\n" % [simulation.tick_ms, simulation.worker_ms, simulation.behind_seconds]
	text += "System: last ms / frame % / rolling p50 / p99 / worst ms\n"
	for system in report:
		if system == "main_thread_engine":
			continue
		var row: Dictionary = report[system]
		text += "%s: %.3f / %.1f%% / %.3f / %.3f / %.3f\n" % [system, row.ms, row.percent, row.p50_ms, row.p99_ms, row.worst_ms]
	text += "PRF-24 runtime metrics (cumulative unless gauge):\n"
	for name: String in runtime:
		text += "%s: %d\n" % [name, runtime[name]]
	var pacing := frame_pacing()
	text += "frame_ms stddev %.3f / p50 %.3f / p99 %.3f / worst %.3f\n" % [pacing.stddev_ms, pacing.p50_ms, pacing.p99_ms, pacing.worst_ms]
	var physics: Dictionary = player.call("physics_frame_breakdown")
	text += "Physics detail: %d steps, %d catch-up, %.3f ms per Jolt step, %d active bodies, %d pairs, %d islands\n" % [physics.steps, physics.catchup_steps, physics.jolt_step_ms, physics.active_bodies, physics.collision_pairs, physics.islands]
	for kind in physics.queries:
		var total_count := 0
		var total_ms := 0.0
		for system in physics.queries[kind]:
			var query_row: Dictionary = physics.queries[kind][system]
			total_count += query_row.count
			total_ms += query_row.ms
		text += "  %s: %d / %.3f ms\n" % [kind, total_count, total_ms]
	label.text = text


func ai_text(report: Dictionary) -> String:
	var text := "F5: AI inclusive us, nested phases overlap\n"
	for level in report.levels:
		var row: Dictionary = report.levels[level]
		text += "%s: %d actors, %d updates, %.1f us, interval %.3f s\n" % [level, row.actors, row.updates, row.us, row.interval_seconds]
	text += "Procedures: "
	var index := 0
	for procedure in report.procedures:
		text += "%s %.1f | " % [procedure, report.procedures[procedure].us]
		index += 1
		if index % 4 == 0:
			text += "\n"
	text += "\nDecisions: "
	index = 0
	for phase in ["package_selection", "schedule_evaluation", "combat_target", "combat_acquisition", "combat_style_choice", "spell_choice", "weapon_choice", "greeting_evaluation", "force_greet_evaluation", "idle_chatter_evaluation", "sandbox_markers", "steering"]:
		text += "%s %.1f | " % [phase, report.phases[phase].us]
		index += 1
		if index % 3 == 0:
			text += "\n"
	text += "\nSTUCK (5 s): " + ", ".join(report.stuck) + "\n"
	text += "Paths: actor requests/s replans/s expanded length latency(us) failures\n"
	index = 0
	for actor in report.paths:
		var row: Dictionary = report.paths[actor]
		var last: Dictionary = row.samples.back() if not row.samples.is_empty() else {"length": 0.0, "latency_us": 0.0}
		text += "%s %d %d %d %.1f %.1f %d\n" % [actor, row.requests_per_second, row.replans_per_second, row.nodes_expanded, last.length, last.latency_us, row.failures]
		index += 1
		if index >= 3:
			break
	text += "Reactions: actor stimulus response latency(us)\n"
	index = 0
	for actor in report.reactions:
		for stimulus in report.reactions[actor]:
			var row: Dictionary = report.reactions[actor][stimulus]
			if row.responses > 0 or row.pending:
				text += "%s %s %s %.1f\n" % [actor, stimulus, "pending" if row.pending else row.response, row.last_us]
				index += 1
				if index >= 3:
					break
		if index >= 3:
			break
	text += "Inactive: middle/low scheduler, combat style choice, idle chatter"
	return text


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


func _capture_simulation() -> Dictionary:
	var status: Dictionary = player.call("simulation_status")
	if status.ready:
		last_simulation = status
	return status


func write_summary() -> Error:
	if not is_instance_valid(player):
		return ERR_UNAVAILABLE
	if not player.call("write_entity_profile", ProjectSettings.globalize_path("user://")):
		return ERR_CANT_CREATE
	var summary: String = player.call("system_summary")
	summary += "\nPRF-24 runtime metrics: " + JSON.stringify(player.call("performance_metrics"))
	var simulation := _capture_simulation()
	if not simulation.ready and not last_simulation.is_empty():
		simulation = last_simulation
	summary += "\nPRF-37 simulation: " + JSON.stringify(simulation)
	summary += "\nPRF-24 frame pacing: " + JSON.stringify(frame_pacing()) + "\n"
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
