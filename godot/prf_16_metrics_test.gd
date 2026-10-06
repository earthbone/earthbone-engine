extends SceneTree

## Deterministic PRF-16 timing-window and full-run percentile acceptance.
const Metrics := preload("res://frame_budget_metrics.gd")


func _initialize() -> void:
	if not ClassDB.class_exists("PlayerNode"):
		printerr("PRF-16 missing release extension")
		quit(1)
		return
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	for method: String in ["begin_budget_frame", "end_budget_frame", "budget_runtime_status"]:
		if not player.has_method(method):
			player.free()
			printerr("PRF-16 missing binding: ", method)
			quit(1)
			return
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	var ok: bool = player.call("end_budget_frame").is_empty()
	ok = ok and player.call("budget_runtime_status").is_empty()
	ok = ok and player.call("begin_budget_frame")
	ok = ok and not player.call("begin_budget_frame")
	var sample: Dictionary = player.call("end_budget_frame")
	ok = ok and sample.frame_ms >= 0.0 and sample.systems.size() == 14
	ok = ok and player.call("end_budget_frame").is_empty()
	var metrics := Metrics.new()
	for i in 3600:
		metrics.observe({"frame_ms": 34.0 if i == 0 else 0.0, "systems": {"ai_planning": 9.0 if i == 0 else 0.0}})
	var row := metrics.report({"scenario": "fixture"})
	ok = ok and row.frame.p50_ms == 0.0 and row.frame.p99_ms == 0.0 and row.frame.worst_ms == 34.0
	ok = ok and row.systems.ai_planning.worst_ms == 9.0 and row.samples == 3600
	player.free()
	print("prf_16_timing_windows_and_full_run_percentiles: ", "PASS" if ok else "FAIL")
	quit(0 if ok else 1)
