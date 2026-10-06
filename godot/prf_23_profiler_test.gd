extends SceneTree

const PROFILER_NAME := "soe_systems"

func _initialize() -> void:
	call_deferred("prf_23_editor_profiler_registers")

func prf_23_editor_profiler_registers() -> void:
	var failures := 0
	if not EngineDebugger.has_profiler(PROFILER_NAME):
		push_error("soe_systems EngineProfiler was not registered")
		failures += 1
	print("prf_23_editor_profiler_registers: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
