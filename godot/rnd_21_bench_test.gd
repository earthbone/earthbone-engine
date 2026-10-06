extends "res://bench.gd"

const LightingChecks := preload("res://rnd_21_lighting_checks.gd")
var checks := LightingChecks.new()
var checking := false

func _process(_delta: float) -> bool:
	if streamed and not checking:
		checking = true
		call_deferred("_check_lighting")
	return false

func _check_lighting() -> void:
	var sky := checks.controller(root)
	if sky == null:
		quit(1)
		return
	sky.set_process(false)
	if bench_radius.is_empty():
		checks.interior(sky, streamer_node.call("cell_lighting", bench_cell))
	else:
		checks.check(sky.records.set_weather("SkyrimClear", true), "benchmark selects vanilla clear weather")
		checks.check(sky.records.set_time(12.0, 0.0), "benchmark noon")
		checks.exterior(sky)
		var noon_direction: Vector3 = sky.sun.basis.z
		var noon_color: Color = sky.sun.light_color
		checks.check(sky.sun.visible, "benchmark noon has a sun")
		checks.check(sky.records.set_time(0.0, 0.0), "benchmark midnight")
		checks.exterior(sky)
		checks.check(not sky.sun.visible and not sky.sun.basis.z.is_equal_approx(noon_direction), "sun moves and disappears at midnight")
		checks.check(sky.sun.light_color != noon_color, "WTHR sunlight changes with time")
		checks.check(sky.records.set_time(12.0, 0.0), "restore benchmark noon")
		checks.check(sky.records.set_weather("SkyrimStormRain", true), "benchmark switches weather")
		checks.exterior(sky)
		sky.records.advance(1.0)
		checks.exterior(sky)
		checks.check(is_equal_approx(sky.last_frame.transition, 1.0), "accelerated weather transition completes")
		checks.check(sky.sun.light_color != noon_color, "sun color changes with weather")
		checks.check(sky.use_streamer(streamer_node, "WhiterunBanneredMare"), "benchmark replaces exterior lighting with interior")
		checks.interior(sky, streamer_node.call("cell_lighting", "WhiterunBanneredMare"))
		checks.check(sky.use_streamer(streamer_node, bench_cell), "benchmark restores exterior lighting")
		checks.exterior(sky)
	checks.controller(root)
	print("RND-21 BENCH: ", "FAIL" if checks.failed else "PASS")
	quit(1 if checks.failed else 0)
