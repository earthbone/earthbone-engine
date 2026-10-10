extends "res://walkthrough.gd"

## HST-23 worker presentation must build every resident NPC without a main-thread World.
var failures := 0

func _configure_player() -> void:
    player.set("capture_mouse", false)
    player.set("auto_step", true)

func _initialize() -> void:
    _watchdog()
    super._initialize()

func _watchdog() -> void:
    await create_timer(150.0).timeout
    printerr("FAIL hst_23_snapshot: watchdog")
    quit(1)

func _start() -> void:
    await super._start()
    if not loaded: return
    system_overlay.queue_free()
    await process_frame
    var deadline := Time.get_ticks_msec() + 90_000
    while Time.get_ticks_msec() < deadline:
        await process_frame
        var status: Dictionary = player.call("simulation_status")
        if not status.error.is_empty():
            printerr(status.error)
            quit(1)
            return
        var counts: Dictionary = player.call("npc_visual_counts")
        if status.ready and counts.expected > 0 and counts.pending == 0 and counts.drawn == counts.expected:
            var has_hulda := false
            for visual: Dictionary in player.call("npc_visuals"):
                has_hulda = has_hulda or visual.editor_id.to_lower().contains("hulda")
                if visual.bones <= 0 or visual.shapes <= 0 or not visual.error.is_empty(): failures += 1
            if not has_hulda: failures += 1
            print("HST-23 snapshots actors=", counts.drawn, " evaluations=", status.evaluations)
            print("RESULT: ", "PASS" if failures == 0 else "FAIL", " hst_23_snapshot")
            quit(0 if failures == 0 else 1)
            return
    printerr("FAIL hst_23_snapshot: all resident actors did not draw ", player.call("npc_visuals"))
    quit(1)
