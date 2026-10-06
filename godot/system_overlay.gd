extends CanvasLayer

## PRF-12 presentation of Rust's fixed system slots. Monitor callbacks and
## units follow Godot's Performance documentation. Sampling reads completed
## frames and never uses Performance.TIME_PROCESS as a frame denominator.

var player: Node3D
var panel: PanelContainer
var label: Label
var elapsed := 0.0
var entities := false
var ai := false
var orphan_elapsed := 0.0
var orphan_start := -1.0
var orphan_warning := false

const MONITORS := {
	"process_ms": Performance.TIME_PROCESS,
	"physics_ms": Performance.TIME_PHYSICS_PROCESS,
	"navigation_ms": Performance.TIME_NAVIGATION_PROCESS,
	"objects": Performance.OBJECT_COUNT,
	"nodes": Performance.OBJECT_NODE_COUNT,
	"orphan_nodes": Performance.OBJECT_ORPHAN_NODE_COUNT,
	"static_memory_bytes": Performance.MEMORY_STATIC,
	"objects_in_frame": Performance.RENDER_TOTAL_OBJECTS_IN_FRAME,
	"primitives_in_frame": Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME,
	"draw_calls_in_frame": Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME,
	"video_memory_bytes": Performance.RENDER_VIDEO_MEM_USED,
	"texture_memory_bytes": Performance.RENDER_TEXTURE_MEM_USED,
	"physics_3d_active_objects": Performance.PHYSICS_3D_ACTIVE_OBJECTS,
	"physics_3d_collision_pairs": Performance.PHYSICS_3D_COLLISION_PAIRS,
	"physics_3d_islands": Performance.PHYSICS_3D_ISLAND_COUNT,
	"audio_output_latency_ms": Performance.AUDIO_OUTPUT_LATENCY,
	"pipeline_canvas": Performance.PIPELINE_COMPILATIONS_CANVAS,
	"pipeline_mesh": Performance.PIPELINE_COMPILATIONS_MESH,
	"pipeline_surface": Performance.PIPELINE_COMPILATIONS_SURFACE,
	"pipeline_draw": Performance.PIPELINE_COMPILATIONS_DRAW,
	"pipeline_specialization": Performance.PIPELINE_COMPILATIONS_SPECIALIZATION,
}


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
	RenderingServer.viewport_set_measure_render_time(get_viewport().get_viewport_rid(), true)
	refresh()
	if is_instance_valid(player):
		entities = player.call("entity_profiling_enabled")
		panel.visible = entities
		player.tree_exiting.connect(_write_on_player_exit)


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
	orphan_elapsed += delta
	if orphan_elapsed >= 10.0:
		var current := float(Performance.get_monitor(Performance.OBJECT_ORPHAN_NODE_COUNT))
		_sample_orphan_count(current)
		orphan_elapsed = 0.0
	elapsed += delta
	if panel.visible and elapsed >= 0.25:
		elapsed = 0.0
		refresh()


func _sample_orphan_count(current: float) -> void:
	orphan_warning = orphan_start >= 0.0 and current > orphan_start
	orphan_start = current


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
	var text := "F3: Engine systems, exclusive scope ms and % of wall frame\n"
	text += "System: last ms / frame % / rolling p50 / p99 / worst ms\n"
	for system in report:
		var row: Dictionary = report[system]
		text += "%s: %.3f / %.1f%% / %.3f / %.3f / %.3f\n" % [system, row.ms, row.percent, row.p50_ms, row.p99_ms, row.worst_ms]
	var godot := godot_metrics()
	text += "Godot ms: process %.3f, physics %.3f, navigation %.3f\n" % [godot.process_ms, godot.physics_ms, godot.navigation_ms]
	text += "Godot objects: %d, nodes: %d, orphans: %d%s\n" % [godot.objects, godot.nodes, godot.orphan_nodes, " WARNING: orphan nodes grew over 10s" if orphan_warning else ""]
	text += "Render visible/shadow/canvas objects: %d / %d / %d, draw calls: %d, primitives: %d\n" % [godot.visible.objects, godot.shadow.objects, godot.canvas.objects, godot.visible.draw_calls, godot.visible.primitives]
	text += "Render CPU/GPU ms: %.3f / %.3f, video/texture MiB: %.1f / %.1f\n" % [godot.render_cpu_ms, godot.render_gpu_ms, godot.video_memory_bytes / 1048576.0, godot.texture_memory_bytes / 1048576.0]
	for kind in ["cells", "plugins"]:
		for name in godot.render_attribution.get(kind, {}):
			var counts: Dictionary = godot.render_attribution[kind][name]
			text += "%s %s: %d instances / %d draw calls / %d primitives\n" % [kind, name, counts.instances, counts.draw_calls, counts.primitives]
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


func godot_metrics() -> Dictionary:
	var values := {}
	for name in MONITORS:
		values[name] = Performance.get_monitor(MONITORS[name])
	var server := RenderingServer
	var rid := get_viewport().get_viewport_rid()
	var passes := {}
	for pair in [["visible", server.VIEWPORT_RENDER_INFO_TYPE_VISIBLE], ["shadow", server.VIEWPORT_RENDER_INFO_TYPE_SHADOW], ["canvas", server.VIEWPORT_RENDER_INFO_TYPE_CANVAS]]:
		passes[pair[0]] = {
			"objects": server.viewport_get_render_info(rid, pair[1], server.VIEWPORT_RENDER_INFO_OBJECTS_IN_FRAME),
			"primitives": server.viewport_get_render_info(rid, pair[1], server.VIEWPORT_RENDER_INFO_PRIMITIVES_IN_FRAME),
			"draw_calls": server.viewport_get_render_info(rid, pair[1], server.VIEWPORT_RENDER_INFO_DRAW_CALLS_IN_FRAME),
		}
	values["visible"] = passes.visible
	values["shadow"] = passes.shadow
	values["canvas"] = passes.canvas
	values["render_cpu_ms"] = server.viewport_get_measured_render_time_cpu(rid)
	values["render_gpu_ms"] = server.viewport_get_measured_render_time_gpu(rid)
	values["frame_setup_cpu_ms"] = server.get_frame_setup_time_cpu()
	values["orphan_growth_warning"] = orphan_warning
	values["render_attribution"] = _render_attribution(get_tree().root)
	return values


func _render_attribution(node: Node) -> Dictionary:
	if node.has_method("render_attribution"):
		return node.call("render_attribution")
	for child in node.get_children():
		var found := _render_attribution(child)
		if not found.is_empty():
			return found
	return {}


func write_summary() -> Error:
	if not is_instance_valid(player):
		return ERR_UNAVAILABLE
	if not player.call("write_entity_profile", ProjectSettings.globalize_path("user://")):
		return ERR_CANT_CREATE
	var summary := JSON.stringify({"systems": player.call("system_summary"), "godot": godot_metrics()})
	print("SOE_FRAME_REPORT " + summary)
	var file := FileAccess.open("user://soe-systems.txt", FileAccess.WRITE)
	if file == null:
		return FileAccess.get_open_error()
	file.store_string(summary)
	return file.get_error()


func _exit_tree() -> void:
	if is_instance_valid(get_viewport()):
		RenderingServer.viewport_set_measure_render_time(get_viewport().get_viewport_rid(), false)


func _write_on_player_exit() -> void:
	var error := write_summary()
	if error != OK:
		printerr("Could not write system summary: ", error_string(error))
