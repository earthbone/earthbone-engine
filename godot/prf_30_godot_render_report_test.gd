extends SceneTree

const Overlay := preload("res://system_overlay.gd")
const CELL := "WhiterunBanneredMare"
var failures := 0


func _initialize() -> void:
	call_deferred("run")


func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-30 FAIL: ", message)


func run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		printerr("PRF-30 requires SKYRIM_DATA")
		quit(1)
		return
	var scene := Node3D.new()
	root.add_child(scene)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	scene.add_child(streamer)
	check(streamer.call("load_data", data), "installed data load failed")
	var built: int = streamer.call("stream_cell", CELL)
	check(built > 0, "installed cell streamed no render instances")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	scene.add_child(player)
	var overlay := Overlay.new()
	overlay.player = player
	scene.add_child(overlay)
	for _frame in 4:
		await process_frame
		await RenderingServer.frame_post_draw
	var metrics: Dictionary = overlay.godot_metrics()
	for name in Overlay.MONITORS:
		check(metrics.has(name), "missing Performance monitor " + name)
		check(is_finite(float(metrics.get(name, NAN))), "non-finite Performance monitor " + name)
	for pass_name in ["visible", "shadow", "canvas"]:
		var pass_data: Dictionary = metrics.get(pass_name, {})
		for field in ["objects", "draw_calls", "primitives"]:
			check(pass_data.has(field), "missing %s %s render info" % [pass_name, field])
	check(metrics.has("render_cpu_ms") and metrics.has("render_gpu_ms"), "render timing fields missing")
	overlay._sample_orphan_count(1.0)
	overlay._sample_orphan_count(2.0)
	check(overlay.orphan_warning, "orphan node growth did not trigger the ten second warning state")
	overlay._sample_orphan_count(2.0)
	check(not overlay.orphan_warning, "stable orphan node count kept the growth warning active")
	var attribution: Dictionary = metrics.get("render_attribution", {})
	var cells: Dictionary = attribution.get("cells", {})
	var plugins: Dictionary = attribution.get("plugins", {})
	check(not cells.is_empty(), "per-cell attribution missing")
	check(not plugins.is_empty(), "per-plugin attribution missing")
	var cell_instances := 0
	for cell_id in cells:
		var counts: Dictionary = cells[cell_id]
		cell_instances += int(counts.instances)
		check(counts.draw_calls == counts.instances, "cell batch draw-call count differs from renderer instances")
	check(cell_instances == built, "per-cell batch counts do not add up to streamed instances")
	var plugin_batches := 0
	for plugin_name in plugins:
		plugin_batches += int(plugins[plugin_name].instances)
	check(plugin_batches >= built, "per-plugin batch attribution omitted cell batches")
	check(overlay.label.text.contains("Render visible/shadow/canvas"), "overlay omits render passes")
	check(overlay.label.text.contains("plugins"), "overlay omits plugin attribution")
	check(overlay.write_summary() == OK, "exit report write failed")
	var summary := FileAccess.get_file_as_string("user://soe-systems.txt")
	check(summary.contains("pipeline_specialization") and summary.contains("render_attribution"), "exit report omits Godot metrics")
	print("PRF_30_RENDER_REPORT ", JSON.stringify({"built": built, "cell_instances": cell_instances, "godot_objects_in_frame": metrics.objects_in_frame, "cells": cells.size(), "plugins": plugins.size()}))
	print("prf_30_godot_render_report: ", "PASS" if failures == 0 else "FAIL")
	DirAccess.remove_absolute(ProjectSettings.globalize_path("user://soe-systems.txt"))
	overlay.free()
	player.free()
	streamer.free()
	scene.free()
	quit(0 if failures == 0 else 1)
