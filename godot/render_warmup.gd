extends RefCounted

## Load-time rendering on stock Godot 4.4. Sources: Godot's pipeline
## compilation guide and RenderingServer's pipeline counters and force_sync.
## Keep the configured camera, lights and materials active under a 2D cover.

const MIN_DRAWN_FRAMES := 3
const QUIET_USEC := 500_000
const TIMEOUT_USEC := 30_000_000
const PIPELINES := [
	RenderingServer.RENDERING_INFO_PIPELINE_COMPILATIONS_CANVAS,
	RenderingServer.RENDERING_INFO_PIPELINE_COMPILATIONS_MESH,
	RenderingServer.RENDERING_INFO_PIPELINE_COMPILATIONS_SURFACE,
	RenderingServer.RENDERING_INFO_PIPELINE_COMPILATIONS_DRAW,
	RenderingServer.RENDERING_INFO_PIPELINE_COMPILATIONS_SPECIALIZATION,
]
const PIPELINE_NAMES := ["canvas", "mesh", "surface", "draw", "specialization"]

var started := 0
var quiet_since := 0
var frames := 0
var previous: Array[int] = []


static func snapshot() -> Array[int]:
	var counts: Array[int] = []
	for monitor in PIPELINES:
		counts.append(RenderingServer.get_rendering_info(monitor))
	return counts


static func named(counts: Array[int]) -> Dictionary:
	var result := {}
	for index in PIPELINE_NAMES.size():
		result[PIPELINE_NAMES[index]] = counts[index]
	return result


static func timing_fields(prepare_started: int, first_drawn_frame: int, settled: int) -> Dictionary:
	return {
		"load_first_frame_wait_ms": (first_drawn_frame - prepare_started) / 1000.0,
		"load_render_ms": (settled - first_drawn_frame) / 1000.0,
	}


func begin(now: int, counts: Array[int]) -> void:
	started = now
	quiet_since = now
	frames = 0
	previous = counts.duplicate()


func observe(now: int, counts: Array[int]) -> bool:
	frames += 1
	if counts != previous:
		quiet_since = now
		previous = counts.duplicate()
	return frames >= MIN_DRAWN_FRAMES and now - quiet_since >= QUIET_USEC


func timed_out(now: int) -> bool:
	return now - started >= TIMEOUT_USEC


static func prepare() -> Dictionary:
	if DisplayServer.get_name() == "headless":
		return {"error": "render warm-up requires a windowed renderer"}
	var wait = load("res://render_warmup.gd").new()
	var prepare_started := Time.get_ticks_usec()
	var before := snapshot()
	var first_drawn_frame := 0
	var last := prepare_started
	var worst_ms := 0.0
	var worst_frame := 0
	while true:
		await RenderingServer.frame_post_draw
		var frame_drawn_at := Time.get_ticks_usec()
		# CONTEXT: Drain submitted GPU work inside loading, never inside measured frames.
		RenderingServer.force_sync()
		var now := Time.get_ticks_usec()
		if first_drawn_frame == 0:
			first_drawn_frame = frame_drawn_at
			wait.begin(first_drawn_frame, before)
			last = now
		var frame_ms := (now - last) / 1000.0
		if frame_ms > worst_ms:
			worst_ms = frame_ms
			worst_frame = wait.frames + 1
		last = now
		var counts := snapshot()
		var ready: bool = wait.observe(now, counts)
		if wait.timed_out(now):
			return {"error": "pipeline counters did not settle within 30 seconds"}
		if ready:
			var report := {
				"error": "",
				"load_render_frames": wait.frames,
				"load_render_worst_ms": worst_ms,
				"load_render_worst_frame": worst_frame,
				"pipelines_before_render": named(before),
				"pipelines_after_load": named(counts),
			}
			report.merge(timing_fields(prepare_started, first_drawn_frame, now))
			return report
	return {"error": "render warm-up interrupted"}


static func cover(view: Window) -> ColorRect:
	var rectangle := ColorRect.new()
	rectangle.color = Color(0.04, 0.04, 0.06)
	rectangle.mouse_filter = Control.MOUSE_FILTER_IGNORE
	view.add_child(rectangle)
	rectangle.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	return rectangle
