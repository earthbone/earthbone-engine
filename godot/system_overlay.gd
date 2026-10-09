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
var plugin_view := false
var ai := false
var orphan_elapsed := 0.0
var orphan_start := -1.0
var orphan_warning := false

const UNMEASURED_RENDER_METRICS := {
	"frustum_culled": "Godot does not expose per-stage cull counts",
	"occlusion_culled": "Godot does not expose per-stage cull counts",
	"visibility_range_culled": "Godot does not expose per-stage cull counts",
	"lod_culled": "Godot does not expose per-stage cull counts",
	"unloaded_cell_culled": "Unloaded references are not counted",
	"lod_switches_per_second": "LOD transitions are not instrumented",
	"shadow_casters_per_light": "Per-light caster counts are not instrumented",
	"materials_created_per_frame": "Material creation is not counted per frame",
	"meshes_created_per_frame": "Mesh creation is not counted per frame",
}

const MONITORS := {
	"fps": Performance.TIME_FPS,
	"process_ms": Performance.TIME_PROCESS,
	"physics_ms": Performance.TIME_PHYSICS_PROCESS,
	"navigation_ms": Performance.TIME_NAVIGATION_PROCESS,
	"objects": Performance.OBJECT_COUNT,
	"nodes": Performance.OBJECT_NODE_COUNT,
	"orphan_nodes": Performance.OBJECT_ORPHAN_NODE_COUNT,
	"static_memory_bytes": Performance.MEMORY_STATIC,
	"static_memory_max_bytes": Performance.MEMORY_STATIC_MAX,
	"message_buffer_max_bytes": Performance.MEMORY_MESSAGE_BUFFER_MAX,
	"resources": Performance.OBJECT_RESOURCE_COUNT,
	"objects_in_frame": Performance.RENDER_TOTAL_OBJECTS_IN_FRAME,
	"primitives_in_frame": Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME,
	"draw_calls_in_frame": Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME,
	"video_memory_bytes": Performance.RENDER_VIDEO_MEM_USED,
	"texture_memory_bytes": Performance.RENDER_TEXTURE_MEM_USED,
	"render_buffer_memory_bytes": Performance.RENDER_BUFFER_MEM_USED,
	"physics_2d_active_objects": Performance.PHYSICS_2D_ACTIVE_OBJECTS,
	"physics_2d_collision_pairs": Performance.PHYSICS_2D_COLLISION_PAIRS,
	"physics_2d_islands": Performance.PHYSICS_2D_ISLAND_COUNT,
	"physics_3d_active_objects": Performance.PHYSICS_3D_ACTIVE_OBJECTS,
	"physics_3d_collision_pairs": Performance.PHYSICS_3D_COLLISION_PAIRS,
	"physics_3d_islands": Performance.PHYSICS_3D_ISLAND_COUNT,
	"navigation_active_maps": Performance.NAVIGATION_ACTIVE_MAPS,
	"navigation_regions": Performance.NAVIGATION_REGION_COUNT,
	"navigation_agents": Performance.NAVIGATION_AGENT_COUNT,
	"navigation_links": Performance.NAVIGATION_LINK_COUNT,
	"navigation_polygons": Performance.NAVIGATION_POLYGON_COUNT,
	"navigation_edges": Performance.NAVIGATION_EDGE_COUNT,
	"navigation_edge_merges": Performance.NAVIGATION_EDGE_MERGE_COUNT,
	"navigation_edge_connections": Performance.NAVIGATION_EDGE_CONNECTION_COUNT,
	"navigation_free_edges": Performance.NAVIGATION_EDGE_FREE_COUNT,
	"navigation_obstacles": Performance.NAVIGATION_OBSTACLE_COUNT,
	"audio_output_latency_ms": Performance.AUDIO_OUTPUT_LATENCY,
	"pipeline_canvas": Performance.PIPELINE_COMPILATIONS_CANVAS,
	"pipeline_mesh": Performance.PIPELINE_COMPILATIONS_MESH,
	"pipeline_surface": Performance.PIPELINE_COMPILATIONS_SURFACE,
	"pipeline_draw": Performance.PIPELINE_COMPILATIONS_DRAW,
	"pipeline_specialization": Performance.PIPELINE_COMPILATIONS_SPECIALIZATION,
}
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
	label.add_theme_constant_override("line_spacing", 0)
	label.add_theme_font_size_override("font_size", 12)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size = Vector2(minf(get_viewport().get_visible_rect().size.x - 16.0, 620.0), minf(get_viewport().get_visible_rect().size.y - 16.0, 500.0))
	panel.add_child(scroll)
	scroll.add_child(label)
	add_child(panel)
	panel.hide()
	RenderingServer.viewport_set_measure_render_time(get_viewport().get_viewport_rid(), true)
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
	elif event is InputEventKey and event.keycode == KEY_F7 and event.pressed and not event.echo:
		ai = false
		plugin_view = not plugin_view
		panel.show()
		refresh()
		get_viewport().set_input_as_handled()


func _process(delta: float) -> void:
	orphan_elapsed += delta
	if orphan_elapsed >= 10.0:
		var current := float(Performance.get_monitor(Performance.OBJECT_ORPHAN_NODE_COUNT))
		_sample_orphan_count(current)
		orphan_elapsed = 0.0
	frame_samples.append(delta * 1000.0)
	if frame_samples.size() > FRAME_SAMPLE_LIMIT:
		frame_samples.pop_front()
	elapsed += delta
	if elapsed >= 0.25:
		elapsed = 0.0
		_capture_simulation()
		if panel.visible:
			refresh()


func _sample_orphan_count(current: float) -> void:
	orphan_warning = orphan_start >= 0.0 and current > orphan_start
	orphan_start = current


func refresh() -> void:
	if not is_instance_valid(player):
		return
	var compact := get_viewport().get_visible_rect().size.y < 400.0
	label.add_theme_font_size_override("font_size", 9 if ai or ((entities or player.has_method("magic_frame_breakdown")) and compact) else 12)
	label.add_theme_constant_override("line_spacing", 0 if compact else 3)
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
	text += "Host phases split render submission, scene CPU excludes server calls\n"
	for system in report:
		if system == "main_thread_engine":
			continue
		var row: Dictionary = report[system]
		text += "%s: %.3f / %.1f%% / %.3f / %.3f / %.3f\n" % [system, row.ms, row.percent, row.p50_ms, row.p99_ms, row.worst_ms]
		if system == "ai_movement" and row.has("phases"):
			var ai_phases: Dictionary = row.phases
			text += "  Last simulation tick:\n"
			for phase in ["scheduling", "actor_driver", "movement", "procedure_tree", "path_following_dispatch"]:
				text += "  %s (inclusive): %.3f ms\n" % [phase, float(ai_phases.get(phase + "_us", 0.0)) / 1000.0]
			for kind in ["navigation", "movement"]:
				text += "  %s workers: %.3f ms busy / %.3f ms caller wait / %d jobs\n" % [kind, float(ai_phases.get(kind + "_worker_busy_us", 0.0)) / 1000.0, float(ai_phases.get(kind + "_worker_wait_us", 0.0)) / 1000.0, int(ai_phases.get(kind + "_worker_jobs", 0))]
			text += "  process high/middle high/middle low/low: %d/%d/%d/%d\n" % [int(ai_phases.get("high_actors", 0)), int(ai_phases.get("middle_high_actors", 0)), int(ai_phases.get("middle_low_actors", 0)), int(ai_phases.get("low_actors", 0))]
	var godot := godot_metrics()
	text += "Godot frame: %.1f fps, %d resources, static memory max %d, message buffer max %d, render buffers %.1f MiB\n" % [godot.fps, godot.resources, godot.static_memory_max_bytes, godot.message_buffer_max_bytes, godot.render_buffer_memory_bytes / 1048576.0]
	text += "Physics 2D: %d objects, %d pairs, %d islands; navigation: %d maps, %d regions, %d agents, %d links, %d polygons\n" % [godot.physics_2d_active_objects, godot.physics_2d_collision_pairs, godot.physics_2d_islands, godot.navigation_active_maps, godot.navigation_regions, godot.navigation_agents, godot.navigation_links, godot.navigation_polygons]
	text += "Godot ms: process %.3f, physics %.3f, navigation %.3f\n" % [godot.process_ms, godot.physics_ms, godot.navigation_ms]
	text += "Godot objects: %d, nodes: %d, orphans: %d%s\n" % [godot.objects, godot.nodes, godot.orphan_nodes, " WARNING: orphan nodes grew over 10s" if orphan_warning else ""]
	text += "Render visible/shadow/canvas objects: %d / %d / %d, draw calls: %d, primitives: %d\n" % [godot.visible.objects, godot.shadow.objects, godot.canvas.objects, godot.visible.draw_calls, godot.visible.primitives]
	text += "Render CPU/GPU ms: %.3f / %.3f, video/texture MiB: %.1f / %.1f\n" % [godot.render_cpu_ms, godot.render_gpu_ms, godot.video_memory_bytes / 1048576.0, godot.texture_memory_bytes / 1048576.0]
	for kind in ["cells", "plugins"]:
		for name in godot.render_attribution.get(kind, {}):
			var counts: Dictionary = godot.render_attribution[kind][name]
			text += "%s %s: %d instances / %d draw calls / %d primitives\n" % [kind, name, counts.instances, counts.draw_calls, counts.primitives]
	if not godot.render_attribution.unavailable.is_empty():
		text += "Render attribution unavailable: " + ", ".join(godot.render_attribution.unavailable.keys()) + "\n"
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
	var magic_phases: Dictionary = player.call("magic_frame_breakdown") if player.has_method("magic_frame_breakdown") else {}
	if not magic_phases.is_empty():
		text += "Magic tick phases, included above: last / p50 / p99 / worst ms\n"
		for phase in magic_phases:
			var row: Dictionary = magic_phases[phase]
			text += "%s: %.3f / %.3f / %.3f / %.3f\n" % [phase, row.ms, row.p50_ms, row.p99_ms, row.worst_ms]
	var health: Dictionary = player.call("papyrus_health")
	var health_display := "Papyrus health, F7: %s\n" % ("per plugin" if plugin_view else "per script")
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
	var attribution := _render_attribution(get_tree().root)
	var unavailable := {}
	for name in UNMEASURED_RENDER_METRICS:
		if not attribution.has(name):
			attribution[name] = null
			unavailable[name] = UNMEASURED_RENDER_METRICS[name]
	attribution["unavailable"] = unavailable
	values["render_attribution"] = attribution
	return values


func _render_attribution(node: Node) -> Dictionary:
	if node.has_method("render_attribution"):
		return node.call("render_attribution")
	for child in node.get_children():
		var found := _render_attribution(child)
		if not found.is_empty():
			return found
	return {}


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
	var simulation := _capture_simulation()
	if not simulation.ready and not last_simulation.is_empty():
		simulation = last_simulation
	var summary := JSON.stringify({"systems": player.call("system_summary"), "godot": godot_metrics(), "runtime_metrics": player.call("performance_metrics"), "simulation": simulation, "frame_pacing": frame_pacing()})
	print("SOE_FRAME_REPORT " + summary)
	var file := FileAccess.open("user://soe-systems.txt", FileAccess.WRITE)
	if file == null:
		return FileAccess.get_open_error()
	file.store_string(summary)
	return file.get_error()


func _exit_tree() -> void:
	if is_instance_valid(get_viewport()):
		RenderingServer.viewport_set_measure_render_time(get_viewport().get_viewport_rid(), false)


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
