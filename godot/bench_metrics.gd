extends RefCounted

## Benchmark budgets and bounded measurement windows. Sources: the local
## Godot Performance/RenderingServer docs and godotwind's benchmarking manual.

var p99_limit := 16.7
var worst_limit := 33.0
var seconds := 5.0
var soak := false
var started := 0
var window_started := 0
var window := 0
var frame_ms: Array[float] = []
var gpu_ms: Array[float] = []
var cpu_ms: Array[float] = []
var worst_frame := 0
var worst_ms := -1.0
var view: Window


func configure() -> String:
	for setting in [["SOE_BENCH_P99_MS", "p99_limit"], ["SOE_BENCH_WORST_MS", "worst_limit"], ["SOE_BENCH_SECONDS", "seconds"]]:
		var raw := OS.get_environment(setting[0])
		if not raw.is_empty():
			if not raw.is_valid_float():
				return "invalid %s" % setting[0]
			var value := float(raw)
			if not is_finite(value) or value <= 0.0:
				return "%s must be finite and positive" % setting[0]
			set(setting[1], value)
	if seconds < 5.0 or seconds > 86400.0:
		return "SOE_BENCH_SECONDS must be 5 to 86400"
	soak = OS.get_environment("SOE_BENCH_SOAK") == "1"
	return ""


static func percentile(values: Array[float], fraction: float) -> float:
	var sorted := values.duplicate()
	sorted.sort()
	return sorted[clampi(int(ceil(sorted.size() * fraction)) - 1, 0, sorted.size() - 1)]


func observe(frame: float, gpu: float, cpu: float, frame_number: int = -1) -> void:
	if started == 0:
		started = Time.get_ticks_usec()
		window_started = started
	frame_ms.append(frame)
	if frame > worst_ms:
		worst_ms = frame
		worst_frame = frame_number if frame_number >= 0 else frame_ms.size()
	gpu_ms.append(gpu)
	cpu_ms.append(cpu)


func ready() -> bool:
	var elapsed := (Time.get_ticks_usec() - window_started) / 1000000.0
	return frame_ms.size() >= 600 and elapsed >= (5.0 if soak else seconds)


func expired() -> bool:
	return (Time.get_ticks_usec() - started) / 1000000.0 >= seconds


func report(extra: Dictionary) -> Dictionary:
	var report_started := Time.get_ticks_usec()
	var row := extra.duplicate()
	row.merge({
		"window": window,
		"samples": frame_ms.size(),
		"elapsed_s": (Time.get_ticks_usec() - started) / 1000000.0,
		"p50_ms": percentile(frame_ms, 0.5),
		"p99_ms": percentile(frame_ms, 0.99),
		"p999_ms": percentile(frame_ms, 0.999),
		"worst_ms": percentile(frame_ms, 1.0),
		"worst_frame": worst_frame,
		"gpu_p99_ms": percentile(gpu_ms, 0.99),
		"render_cpu_p99_ms": percentile(cpu_ms, 0.99),
		"rss_mib": null,
		"godot_static_mib": Performance.get_monitor(Performance.MEMORY_STATIC) / 1048576.0,
		"video_mib": Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED) / 1048576.0,
		"texture_mib": Performance.get_monitor(Performance.RENDER_TEXTURE_MEM_USED) / 1048576.0,
		"draw_calls": Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME),
		"primitives": Performance.get_monitor(Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME),
		"godot_version": Engine.get_version_info()["string"],
		"renderer": RenderingServer.get_current_rendering_method(),
		"adapter": RenderingServer.get_video_adapter_name(),
		"window_size": [DisplayServer.window_get_size().x, DisplayServer.window_get_size().y],
		"render_size": [view.get_texture().get_width(), view.get_texture().get_height()] if view != null else [],
	}, true)
	row["report_cpu_ms"] = (Time.get_ticks_usec() - report_started) / 1000.0
	print("SOE_SAMPLE " + JSON.stringify(row))
	return row


func within_budget(row: Dictionary) -> bool:
	return is_finite(row["p99_ms"]) and is_finite(row["worst_ms"]) and row["p99_ms"] <= p99_limit and row["worst_ms"] <= worst_limit


func next_window() -> void:
	window += 1
	window_started = Time.get_ticks_usec()
	frame_ms.clear()
	gpu_ms.clear()
	cpu_ms.clear()
	worst_ms = -1.0
	worst_frame = 0


static func rendered_work(row: Dictionary) -> bool:
	return row.get("draw_calls", 0) > 0 and row.get("primitives", 0) > 0


func fixed_viewport(viewport: Window) -> void:
	view = viewport
	viewport.content_scale_size = Vector2i(1600, 900)
	viewport.content_scale_mode = Window.CONTENT_SCALE_MODE_VIEWPORT
