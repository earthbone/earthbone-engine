extends SceneTree

const Budget := preload("res://simulation_budget.gd")


func _initialize() -> void:
	var passing: Array[float] = [1.0, 2.0]
	var p50_failure: Array[float] = [2.01, 2.01]
	var p99_failure: Array[float] = []
	p99_failure.resize(100)
	p99_failure.fill(1.0)
	p99_failure[98] = 4.01
	p99_failure[99] = 4.01
	var ok: bool = Budget.evaluate(passing, 1.0, 60).passed
	ok = ok and not Budget.evaluate(p50_failure, 0.0, 60).passed
	ok = ok and not Budget.evaluate(p99_failure, 0.0, 60).passed
	ok = ok and not Budget.evaluate(passing, 1.001, 60).passed
	ok = ok and not Budget.evaluate(passing, 0.0, 0).passed
	ok = ok and not Budget.evaluate([], 0.0, 60).passed
	var invalid: Array[float] = [NAN, 1.0]
	ok = ok and not Budget.evaluate(invalid, 0.0, 60).passed
	print("prf_37_tst_18_rejects_main_thread_percentiles_and_simulation_lag: ", "PASS" if ok else "FAIL")
	quit(0 if ok else 1)
