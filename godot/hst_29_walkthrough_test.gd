extends "res://walkthrough.gd"

## HST-29 runs the TST-19 NPC checks under normal gameplay, without moving NPCs
## from the test. The gate lasts three minutes. Placement error stays below 8 units.
const NpcChecks := preload("res://tst_19_npc_checks.gd")
const METRES_PER_UNIT := 0.0142875
var failures := 0
var max_error := 0.0
var samples := 0

func _initialize() -> void:
    _watchdog()
    super._initialize()

func _watchdog() -> void:
    await create_timer(360.0).timeout
    printerr("FAIL hst_29: watchdog")
    quit(1)

func _configure_player() -> void:
    player.set("capture_mouse", false)
    player.set("auto_step", true)

func _start() -> void:
    await super._start()
    if not loaded: return
    system_overlay.queue_free()
    var deadline := Time.get_ticks_msec() + 90_000
    while Time.get_ticks_msec() < deadline:
        await process_frame
        var counts: Dictionary = player.call("npc_visual_counts")
        if counts.expected > 0 and counts.pending == 0 and counts.drawn == counts.expected:
            break
    _frame_npc()
    await process_frame
    await RenderingServer.frame_post_draw
    root.get_texture().get_image().save_png("res://.godot/hst_29_motion_0.png")
    var captured := false
    _diagnostics()
    var checks := NpcChecks.new()
    var started := Time.get_ticks_msec()
    var duration := 180_000 if OS.get_environment("SOE_PLAY_PATH") == "gate" else 30_000
    var tried_market := false
    var movement_reported := false
    while Time.get_ticks_msec() - started < duration:
        await process_frame
        await RenderingServer.frame_post_draw
        checks.sample(player, Time.get_ticks_msec())
        if not movement_reported and Time.get_ticks_msec() - checks.started_msec >= NpcChecks.WINDOW_MSEC:
            print("HST-29 movement window units=", checks.max_units, " walk_frames=", checks.walk_frames)
            movement_reported = true
        if duration > 30_000 and not tried_market and Time.get_ticks_msec() - started >= 30_000:
            tried_market = true
            if checks.max_units < NpcChecks.MOVE_UNITS:
                if not await _enter_market():
                    failures += 1
                checks.restart_movement_window(Time.get_ticks_msec())
                movement_reported = false
        if not captured and Time.get_ticks_msec() - started >= 1000:
            root.get_texture().get_image().save_png("res://.godot/hst_29_motion_1.png")
            captured = true
        for visual: Dictionary in player.call("npc_visuals"):
            if visual.node_id == 0: continue
            var pose: Dictionary = player.call("gameplay_reference_pose", visual.file, visual.object)
            if pose.is_empty():
                failures += 1
                continue
            var runtime: Vector3 = pose.position
            var expected := Vector3(runtime.x, runtime.z, -runtime.y) * METRES_PER_UNIT
            max_error = maxf(max_error, expected.distance_to(visual.position) / METRES_PER_UNIT)
            samples += 1
    _diagnostics()
    var passed: bool = checks.report()
    passed = passed and samples > 0 and max_error <= 8.0 and failures == 0
    print("PLAY_PATH npc.transform ", "pass" if max_error <= 8.0 and samples > 0 else "fail", " max_units=", max_error, " samples=", samples)
    print("HST-29 visuals ", player.call("npc_visuals"))
    print("RESULT: ", "PASS" if passed else "FAIL", " hst_29")
    quit(0 if passed else 1)

func _enter_market() -> bool:
    var previous_nodes := {}
    for visual: Dictionary in player.call("npc_visuals"):
        if visual.node_id != 0:
            previous_nodes[visual.node_id] = true
    for door: Dictionary in player.call("gameplay_doors"):
        if door.locked or door.get("far_worldspace", "") != "WhiterunWorld": continue
        if not player.call("activate_reference", door.file, door.object): continue
        player.call("advance", 0.0)
        var deadline := Time.get_ticks_msec() + 90_000
        while Time.get_ticks_msec() < deadline:
            await process_frame
            var counts: Dictionary = player.call("npc_visual_counts")
            var departed := true
            for visual: Dictionary in player.call("npc_visuals"):
                if previous_nodes.has(visual.node_id):
                    departed = false
                    break
            if player.call("detection_worldspace") == "WhiterunWorld" and departed and counts.expected > 0 and counts.pending == 0 and counts.drawn == counts.expected:
                _frame_npc()
                print("HST-29 measuring market NPCs")
                return true
        return false
    return false

func _frame_npc() -> void:
    for visual: Dictionary in player.call("npc_visuals"):
        if visual.node_id == 0 or visual.shapes == 0: continue
        var center: Vector3 = visual.position + Vector3.UP * 0.85
        var camera := Camera3D.new()
        root.add_child(camera)
        var clearance := 0.0
        var space := player.get_world_3d().direct_space_state
        for angle in range(12):
            var direction := Vector3(sin(angle * TAU / 12.0), 0.15, cos(angle * TAU / 12.0)).normalized()
            var hit := space.intersect_ray(PhysicsRayQueryParameters3D.create(center, center + direction * 2.0))
            var distance := 2.0 if hit.is_empty() else center.distance_to(hit.position) - 0.15
            if distance > clearance:
                clearance = distance
                camera.position = center + direction * distance
        if clearance <= 0.1:
            camera.queue_free()
            continue
        camera.look_at(center)
        camera.current = true
        return

func _diagnostics() -> void:
    if player.has_method("npc_package_states"):
        print("HST-29 packages ", player.call("npc_package_states"))
    print("HST-29 AI ", player.call("ai_planning_details"), " evaluations=", player.call("ai_package_evaluations"))
    for visual: Dictionary in player.call("npc_visuals"):
        if visual.node_id == 0: continue
        var actor: Node = instance_from_id(visual.node_id)
        print("HST-29 graph ", visual.editor_id, " elapsed=", actor.call("elapsed"), " stats=", actor.call("stats"),
            " behavior=", actor.call("behavior_path"), " clips=", actor.call("active_clips"), " states=", actor.call("active_states"))
