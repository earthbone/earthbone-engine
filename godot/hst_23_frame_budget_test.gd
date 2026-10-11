extends "res://tst_18_walkthrough.gd"

## HST-23 uses TST-18's declared 60 second, p99 16.7 ms, worst 33 ms gate.
## TST-18 requires five runtime actors including the player, hence four NPC views.
var sync_samples: Array[float] = []
var pose_samples: Array[float] = []
var npcs_ready := false

func _initialize() -> void:
    startup_cell = "WhiterunOrigin"
    startup_radius = 1
    startup_position = Vector3(2048.0, 2048.0, 500.0)
    super._initialize()

func _start() -> void:
    await super._start()
    if not loaded: return
    system_overlay.queue_free()
    await process_frame
    var deadline := Time.get_ticks_msec() + 90_000
    while Time.get_ticks_msec() < deadline:
        await process_frame
        var counts: Dictionary = player.call("npc_visual_counts")
        if counts.expected >= 4 and counts.pending == 0 and counts.drawn == counts.expected and counts.failed == 0:
            ready_usec = Time.get_ticks_usec()
            npcs_ready = true
            return
        if counts.failed > 0:
            printerr("HST-23 FAIL NPC loading ", player.call("npc_visuals"))
            quit(1)
            return
    printerr("HST-23 FAIL NPC loading watchdog ", player.call("npc_visual_counts"))
    quit(1)

func _process(delta: float) -> bool:
    if not npcs_ready: return false
    if loaded:
        var timing: Dictionary = player.call("npc_visual_timing")
        sync_samples.append(timing.sync_us)
        pose_samples.append(timing.pose_us)
    return super._process(delta)

func _finish(now: int) -> void:
    var counts: Dictionary = player.call("npc_visual_counts")
    var present: bool = counts.expected >= 4 and counts.drawn == counts.expected and counts.failed == 0 and counts.pending == 0
    if not present:
        printerr("HST-23 FAIL NPC set ", counts, " ", player.call("npc_visuals"))
        violations += 1
    print("HST-23 frame actors=", counts.drawn, " sync_p50_us=", Metrics.percentile(sync_samples, 0.5), " sync_p99_us=", Metrics.percentile(sync_samples, 0.99),
        " pose_p50_us=", Metrics.percentile(pose_samples, 0.5), " pose_p99_us=", Metrics.percentile(pose_samples, 0.99))
    super._finish(now)
