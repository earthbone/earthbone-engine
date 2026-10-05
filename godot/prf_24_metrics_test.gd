extends SceneTree

const Overlay := preload("res://system_overlay.gd")
const METRICS := [
	"detection_checks", "detection_pairs", "line_of_sight_raycasts",
	"path_cache_hits", "path_cache_misses", "physics_bodies", "physics_contacts",
	"audio_voices", "audio_decode_ns", "audio_footstep_latency_ns",
	"audio_voice_latency_ns", "audio_spell_latency_ns", "audio_impact_latency_ns",
	"audio_output_latency_ns", "animation_actors", "animation_behavior_ns",
	"animation_sampling_ns", "animation_blending_ns", "animation_ik_ns",
	"animation_skinning_ns", "animation_event_latency_ns",
	"animation_game_time_drift_ns", "frame_time_stddev_ns", "frame_time_p50_ns",
	"frame_time_p99_ns", "frame_time_worst_ns", "input_to_screen_latency_ns",
]
var failures := 0


func _initialize() -> void:
	call_deferred("prf_24_metrics_report_overlay_and_summary")


func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-24 FAIL: ", message)


func prf_24_metrics_report_overlay_and_summary() -> void:
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	var overlay := Overlay.new()
	overlay.player = player
	get_root().add_child(overlay)
	for frame in [0.010, 0.020, 0.030, 0.040]:
		overlay._process(frame)
	var pacing: Dictionary = overlay.frame_pacing()
	check(is_equal_approx(pacing.p50_ms, 20.0), "frame p50")
	check(is_equal_approx(pacing.p99_ms, 30.0), "frame p99")
	check(is_equal_approx(pacing.worst_ms, 40.0), "frame worst")
	check(pacing.stddev_ms > 0.0, "frame standard deviation")
	var metrics: Dictionary = player.call("performance_metrics")
	for name in METRICS:
		check(metrics.has(name), "missing metric " + name)
	overlay.refresh()
	for name in METRICS:
		check(overlay.label.text.contains(name), "overlay missing " + name)
	check(overlay.write_summary() == OK, "summary write failed")
	var summary := FileAccess.get_file_as_string("user://soe-systems.txt")
	for name in METRICS:
		check(summary.contains(name), "exit summary missing " + name)
	DirAccess.remove_absolute(ProjectSettings.globalize_path("user://soe-systems.txt"))
	player.free()
	overlay.free()
	print("prf_24_metrics_report_overlay_and_summary: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
