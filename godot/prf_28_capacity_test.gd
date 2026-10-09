extends SceneTree

## Deterministic ladder checks, separate from windowed throughput evidence.

const Ladder := preload("res://capacity_curve.gd")
var failures := 0


func check(value: bool, name: String) -> void:
	if not value:
		failures += 1
		printerr("FAIL " + name)
	else:
		print("PASS " + name)


func _initialize() -> void:
	var curve := Ladder.new()
	check(not curve.configure(0, 8).is_empty(), "prf_28_invalid_light_counts_rejected")
	check(curve.configure(3, 7).is_empty(), "prf_28_valid_light_ladder")
	curve.observe(true)
	check(curve.load_count == 6, "prf_28_light_ramp_doubles")
	curve.observe(true)
	check(curve.load_count == 7, "prf_28_light_ramp_clamps_to_max")
	curve.observe(true)
	check(curve.finished and curve.result().status == "limit_not_found", "prf_28_unreached_light_limit_reported")
	var failed := Ladder.new()
	failed.configure(1, 256)
	failed.observe(false)
	failed.observe(true)
	check(failed.result().max_passing_rung == 0 and failed.result().first_failing_rung == 1, "prf_28_light_failure_stops_curve")
	quit(0 if failures == 0 else 1)
