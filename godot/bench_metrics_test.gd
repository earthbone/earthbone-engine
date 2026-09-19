extends SceneTree

## Deterministic checks of percentile and budget logic, no renderer required.
const Metrics := preload("res://bench_metrics.gd")


func _initialize() -> void:
	var metrics := Metrics.new()
	var values: Array[float] = []
	for n in 100:
		values.append(float(n + 1))
	if Metrics.percentile(values, 0.99) != 99.0:
		quit(1)
		return
	if metrics.within_budget({"p99_ms": 17.0, "worst_ms": 20.0}):
		quit(1)
		return
	if metrics.within_budget({"p99_ms": 1.0, "worst_ms": 34.0}):
		quit(1)
		return
	if metrics.within_budget({"p99_ms": NAN, "worst_ms": 1.0}):
		quit(1)
		return
	if not metrics.within_budget({"p99_ms": 16.7, "worst_ms": 33.0}):
		quit(1)
		return
	if Metrics.rendered_work({"draw_calls": 0, "primitives": 0}):
		quit(1)
		return
	OS.set_environment("SOE_BENCH_P99_MS", "typo")
	if metrics.configure().is_empty():
		quit(1)
		return
	print("RESULT: PASS: percentile, budget and invalid configuration checks")
	quit(0)
