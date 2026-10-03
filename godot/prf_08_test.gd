extends SceneTree

## Deterministic PRF-08 loading and reporting contracts. No GPU timing claim.
const Warmup := preload("res://render_warmup.gd")
const Metrics := preload("res://bench_metrics.gd")
const Runs := preload("res://bench_runs.gd")
var failures := 0


func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr(message)


func prf_08_loading_requires_drawn_frames_and_quiet_pipelines() -> void:
	var wait := Warmup.new()
	var counts: Array[int] = [0, 2, 3, 0, 4]
	wait.begin(10, counts)
	check(not wait.observe(500_010, counts), "one drawn frame cannot finish loading")
	check(not wait.observe(500_011, counts), "two drawn frames cannot finish loading")
	check(wait.observe(500_012, counts), "three drawn frames and quiet counters finish loading")
	check(Warmup.named(counts).surface == 3, "surface compilations retain their identity")


func prf_08_late_compilation_restarts_quiet_interval_and_timeout_fails() -> void:
	var wait := Warmup.new()
	var counts: Array[int] = [0, 0, 0, 0, 0]
	wait.begin(1, counts)
	for index in 5:
		check(not wait.observe(100_001 + index, counts), "short quiet interval cannot finish")
	for index in counts.size():
		counts[index] += 1
		var now := 600_001 * (index + 1)
		check(not wait.observe(now, counts), "each compilation category resets quiet time")
		check(not wait.observe(now + 499_999, counts), "late compilation needs full quiet interval")
	check(wait.observe(3_500_005, counts), "quiet interval eventually finishes")
	check(not wait.timed_out(30_000_000), "timeout must respect the start timestamp")
	check(wait.timed_out(30_000_001), "unsettled rendering fails at thirty seconds")


func prf_08_worst_frame_is_first_maximum_and_resets_per_window() -> void:
	var metrics := Metrics.new()
	metrics.observe(1.0, 0.1, 0.1, 1)
	metrics.observe(69.0, 0.1, 0.1, 2)
	metrics.observe(69.0, 0.1, 0.1, 3)
	check(metrics.worst_frame == 2 and metrics.worst_ms == 69.0, "worst frame retains first tie")
	check(not metrics.within_budget({"p99_ms": 1.0, "worst_ms": metrics.worst_ms}), "69 ms hitch fails")
	metrics.next_window()
	metrics.observe(2.0, 0.1, 0.1, 8)
	check(metrics.worst_frame == 8 and metrics.worst_ms == 2.0, "new window resets maximum")
	check(metrics.within_budget({"p99_ms": 1.0, "worst_ms": 32.999}), "under 33 ms passes")


func prf_08_cold_start_arguments_use_fresh_children() -> void:
	var config := Runs.configure(PackedStringArray(["--cold-starts", "3"]))
	check(config.error.is_empty() and config.launcher and config.runs == 3, "three cold starts accepted")
	var args := Runs.child_args("/project with spaces", 2)
	var separator := args.find("--")
	var child := Runs.configure(args.slice(separator + 1))
	check(args[1] == "/project with spaces", "project path remains one argument")
	check(child.error.is_empty() and not child.launcher and child.run == 2, "child cannot relaunch parent")
	for invalid in [
		["--cold-starts"], ["--cold-starts", "no"], ["--cold-starts", "0"],
		["--cold-starts", "129"], ["--cold-starts", "3", "--cold-run", "1"],
		["--cold-starts", "3", "--cold-starts", "3"], ["--unknown", "3"],
	]:
		check(not Runs.configure(PackedStringArray(invalid)).error.is_empty(), "invalid repetition rejected")


func prf_08_repetition_failure_cannot_be_hidden_by_later_pass() -> void:
	check(Runs.child_status(0, "no report") == 1, "empty success fails")
	check(Runs.child_status(0, "SCRIPT ERROR: bad\nRESULT: PASS") == 1, "script error fails")
	check(Runs.child_status(2, "RESULT: FAIL: frame budget exceeded") == 2, "budget failure retained")
	check(Runs.child_status(0, "RESULT: PASS") == 0, "valid pass accepted")
	var status := 0
	for code in [2, 0, 0]:
		status = Runs.combined_status(status, code)
	check(status == 2, "later pass cannot hide first-run hitch")
	check(Runs.combined_status(status, 1) == 1, "invalid measurement outranks budget breach")


func _initialize() -> void:
	prf_08_loading_requires_drawn_frames_and_quiet_pipelines()
	prf_08_late_compilation_restarts_quiet_interval_and_timeout_fails()
	prf_08_worst_frame_is_first_maximum_and_resets_per_window()
	prf_08_cold_start_arguments_use_fresh_children()
	prf_08_repetition_failure_cannot_be_hidden_by_later_pass()
	print("RESULT: %s: prf_08 deterministic checks" % ("PASS" if failures == 0 else "FAIL"))
	quit(0 if failures == 0 else 1)
