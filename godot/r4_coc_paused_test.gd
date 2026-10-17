extends "res://walkthrough.gd"

var failures: Array[String] = []

func _configure_player() -> void:
    root.size = Vector2i(640, 360)
    player.set("capture_mouse", false)

func _spawn_position(_source: Node3D) -> Vector3:
    return Vector3(-125, -800, 120)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures.append(message)
        printerr("R4_PAUSED failure: ", message)

func _start() -> void:
    await super._start()
    if not loaded:
        return
    player.call("console_set_open", true)
    var days_before: float = player.call("gameplay_days")
    var result := str(player.call("console_execute", "coc QASmoke"))
    _check(not result.contains("error:"), "queued jump is accepted: " + result)
    var deadline := Time.get_ticks_msec() + 20000
    while player.call("is_location_preparing") and Time.get_ticks_msec() < deadline:
        await process_frame
    print("R4_PAUSED_INSTALL_MS ", Time.get_ticks_msec() - (deadline - 20000))
    _check(not player.call("is_location_preparing"), "queued cell installs while console stays open")
    _check(player.call("console_is_open"), "console remains open during jump")
    _check(is_equal_approx(player.call("gameplay_days"), days_before), "jump does not advance paused game clock")
    var position: Vector3 = player.call("position")
    _check(position.distance_to(Vector3(-363.26642, 2035.2379, 6976)) < 0.01, "queued jump installs QASmoke marker pose: " + str(position))
    for frame_index in range(3):
        await process_frame
    var expected: Dictionary = streamer.call("cell_lighting", "QASmoke")
    var environment := root.get_camera_3d().environment
    _check(environment != null, "destination has its camera imagespace environment")
    if environment != null:
        _check(environment.ambient_light_color == expected.ambient, "camera uses destination ambient lighting")
        _check(environment.fog_light_color == expected.fog_near_color, "camera uses destination fog color")
        _check(is_equal_approx(environment.fog_depth_end, expected.fog_far / 70.0), "camera uses destination fog distance")
    print("R4_PAUSED_RESULT ", "PASS" if failures.is_empty() else "FAIL", " failures=", failures)
    quit(0 if failures.is_empty() else 1)
