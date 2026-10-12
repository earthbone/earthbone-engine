extends SceneTree

const Route = preload("res://prf_31_walk_route.gd")
var failures := 0

func _check(ok: bool, name: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", name)

func _initialize() -> void:
	var route = Route.new(0.0, Vector3.ZERO)
	route.update(Vector3.ZERO, 0.25)
	_check(route.direction().is_equal_approx(Vector3(0, 1, 0)), "prf_31_walk_keeps_heading_before_stall_probe")
	route.update(Vector3.ZERO, 0.25)
	_check(route.direction().is_equal_approx(Vector3(1, 0, 0)), "prf_31_walk_turns_when_collision_stops_progress")
	route.update(Vector3(100, 0, 0), 0.5)
	_check(route.direction().is_equal_approx(Vector3(1, 0, 0)), "prf_31_walk_keeps_heading_while_moving")
	route.update(Vector3(1000, 0, 0), 4.5)
	_check(route.direction().is_equal_approx(Vector3(0, -1, 0)), "prf_31_walk_turns_after_five_seconds")
	for _i in range(3):
		route.update(Vector3(1000, 0, 0), 0.5)
	_check(route.direction().is_equal_approx(Vector3(1, 0, 0)), "prf_31_walk_probes_all_headings_when_trapped")
	route.update(Vector3.ZERO, -1.0)
	_check(route.direction().is_equal_approx(Vector3(1, 0, 0)), "prf_31_walk_rejects_invalid_elapsed")
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " PRF31 walk route")
	quit(0 if failures == 0 else 1)
