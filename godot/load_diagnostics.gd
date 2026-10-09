extends Node

## PRF-29 pipeline observations on stock Godot 4.4. Sources: local
## godot-docs pipeline_compilations.rst and godotwind pipeline_compile_monitor.gd.
## Counter changes locate a frame interval, not compiler duration or an asset.

const Warmup := preload("res://render_warmup.gd")
const HISTORY := 240
const FRAME_BUDGET_MS := 33.0

var streamer: Node3D
var observations: Array[Dictionary] = []
var previous: Array[int] = []
var last_usec := 0


func _ready() -> void:
	previous = Warmup.snapshot()
	last_usec = Time.get_ticks_usec()
	RenderingServer.frame_post_draw.connect(_drawn)
	tree_exiting.connect(write_dump)


func _drawn() -> void:
	var now := Time.get_ticks_usec()
	var counts := Warmup.snapshot()
	var changes: Array[int] = []
	var changed := false
	for index in counts.size():
		changes.append(maxi(0, counts[index] - previous[index]))
		changed = changed or changes[index] > 0
	var interval_ms := (now - last_usec) / 1000.0
	if changed:
		var report := {
			"cause": "pipeline_compile_observed",
			"interval_ms": interval_ms,
			"compiler_ms": null,
			"asset": "unavailable",
			"sources": Warmup.named(changes),
			"over_budget": interval_ms > FRAME_BUDGET_MS,
		}
		observations.append(report)
		if observations.size() > HISTORY:
			observations.pop_front()
		print("LOAD_COMPILE_OBSERVATION ", JSON.stringify(report))
	previous = counts
	last_usec = now


func summary() -> String:
	var result := "PIPELINE_COUNTS " + JSON.stringify(Warmup.named(Warmup.snapshot())) + "\n"
	result += "shader_compile_ms=unavailable pipeline_compile_ms=unavailable compiler_asset=unavailable\n"
	for report in observations:
		result += "LOAD_COMPILE_OBSERVATION " + JSON.stringify(report) + "\n"
	return result


func write_dump() -> void:
	if not is_instance_valid(streamer):
		return
	var file := FileAccess.open("user://soe-loads.txt", FileAccess.WRITE)
	if file == null:
		printerr("load diagnostic dump: ", FileAccess.get_open_error())
		return
	file.store_string(streamer.call("load_timing_dump") + summary())
	file.close()
	streamer.call("write_load_trace", ProjectSettings.globalize_path("user://soe-loads.json"))
