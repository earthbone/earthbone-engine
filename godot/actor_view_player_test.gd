extends SceneTree

## Fixed before the first run: one unit of eye-height error, 0.001 metre
## of rig-placement error, zero visible body surfaces and zero arm head parts.
const METRES_PER_UNIT := 0.0142875
var failures := 0

func _initialize() -> void:
    call_deferred("_run")

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL player view: ", message)

func _rig_check(player: Node3D, body: Node3D, arms: Node3D) -> void:
    var camera := root.get_camera_3d()
    var third: Dictionary = body.call("play_path_observation")
    var first: Dictionary = arms.call("play_path_observation")
    var expected_height: float = third.head_height * third.placement.basis.y.length()
    var feet: Vector3 = player.call("position")
    var height: float = camera.global_position.y / METRES_PER_UNIT - feet.z
    _check(expected_height > 100.0, "installed race head is above the capsule")
    _check(absf(height - expected_height) <= 1.0, "eye follows race head: " + str([height, expected_height]))
    _check(third.placement.basis.y.normalized().distance_to(Vector3.UP) <= 0.001, "body stays upright while looking down")
    for shape: Dictionary in third.shapes:
        _check(not shape.visible, "third-person body hidden: " + str(shape.name))
    _check(not first.shapes.is_empty(), "first-person arms remain")
    for shape: Dictionary in first.shapes:
        _check(shape.head_part_type == -1, "first-person rig excludes head and hair: " + str(shape.name))
    var eye: Transform3D = first.placement * first.camera_offset
    _check(eye.origin.distance_to(camera.global_position) <= 0.001, "submitted arms stay at the camera")
    for axis in 3:
        _check(eye.basis[axis].distance_to(camera.global_basis[axis]) <= 0.001, "submitted arms keep camera rotation: " + str(axis))
    print("PLAYER_VIEW height=", height, " head=", expected_height, " rig_error=", eye.origin.distance_to(camera.global_position))

func _run() -> void:
    _check(OS.get_environment("DISPLAY") == ":99", "private Xvfb display")
    var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
    root.add_child(streamer)
    _check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "installed data loads")
    var player: Node3D = ClassDB.instantiate("PlayerNode")
    player.set("capture_mouse", false)
    player.set("auto_step", false)
    root.add_child(player)
    _check(player.call("use_streamer_data", streamer), "shared source binds")
    _check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "inn enters")
    _check(player.call("start_gameplay"), "game starts")
    var body: Node3D = ClassDB.instantiate("SkinnedActorNode")
    body.set("playing", false)
    player.add_child(body)
    _check(player.call("bind_actor_appearance", body, 0, 0x14), "body binds")
    var arms: Node3D = ClassDB.instantiate("SkinnedActorNode")
    arms.set("playing", false)
    arms.set("first_person", true)
    player.add_child(arms)
    _check(player.call("bind_actor_appearance", arms, 0, 0x14), "first-person rig binds")
    for facing in [Vector2(0.0, 0.0), Vector2(0.7, -0.8)]:
        player.call("set_facing", facing.x, facing.y)
        player.call("advance", 1.0 / 60.0)
        body.call("advance", 0.0)
        arms.call("advance", 0.0)
        _rig_check(player, body, arms)
    var initial: Vector3 = player.call("position")
    var key := InputEventKey.new()
    key.keycode = KEY_W
    key.physical_keycode = KEY_W
    key.pressed = true
    Input.parse_input_event(key)
    await process_frame
    for frame in 60:
        player.call("advance", 1.0 / 60.0)
        arms.call("advance", 1.0 / 60.0)
    key.pressed = false
    Input.parse_input_event(key)
    await process_frame
    _check(initial.distance_to(player.call("position")) > 16.0, "forward movement happened")
    _rig_check(player, body, arms)
    body.free()
    arms.free()
    player.free()
    streamer.free()
    await process_frame
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " player view")
    quit(0 if failures == 0 else 1)
