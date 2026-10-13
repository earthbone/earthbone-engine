extends "res://walkthrough.gd"

## Regression over the real walkthrough, with no preinstalled collision or UI.
const STEP := 1.0 / 60.0
const UNITS := 69.99125
var failures := 0

func _configure_player() -> void:
    player.set("capture_mouse", true)
    player.set("auto_step", false)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL freewalk_hover: ", message)

func _tick(count := 1) -> void:
    for i in count:
        await physics_frame
        player.call("advance", STEP)

func _key(code: Key, pressed: bool) -> void:
    var event := InputEventKey.new()
    event.keycode = code
    event.pressed = pressed
    Input.parse_input_event(event)
    Input.flush_buffered_events()

func _aim(reference: Dictionary) -> bool:
    for offset: Vector3 in [Vector3(0, -100, 5), Vector3(-100, 0, 5), Vector3(100, 0, 5), Vector3(0, 100, 5)]:
        player.call("queue_player_input", {"action": "move_to", "file": reference.file, "object": reference.object, "offset": offset, "match_rotation": false})
        player.call("step_player_input")
        await _tick(30)
        var pose: Dictionary = player.call("gameplay_reference_pose", reference.file, reference.object)
        var eye_godot := get_root().get_camera_3d().global_position
        var eye := Vector3(eye_godot.x, -eye_godot.z, eye_godot.y) * UNITS
        var direction: Vector3 = pose.position + Vector3(0, 0, 30) - eye
        player.call("set_facing", atan2(direction.x, direction.y), -atan2(direction.z, Vector2(direction.x, direction.y).length()))
        await _tick(2)
        var status: Dictionary = player.call("carry_status")
        if status.get("crosshair_file", -1) == reference.file and status.get("crosshair_object", -1) == reference.object:
            return true
    return false

func _start() -> void:
    create_timer(180.0).timeout.connect(func() -> void:
        printerr("FAIL freewalk_hover: watchdog")
        quit(1))
    await super._start()
    if not loaded: return
    var container: Dictionary = {}
    for candidate: Dictionary in player.call("gameplay_containers"):
        if await _aim(candidate):
            container = candidate
            break
    _check(not container.is_empty(), "crosshair acquires an installed container")
    if container.is_empty():
        quit(1)
        return
    await process_frame
    await process_frame
    _check(not player.call("is_menu_open"), "hover leaves menus closed")
    _check(not player.call("quick_loot_snapshot").open, "hover shows prompt without QuickLoot")
    _check(player.get_node_or_null("QuickLootLayer") == null, "hover allocates no preview viewport")
    _check(player.call("activation_prompt").get("action", "") == "Open", "hover shows Open prompt")
    _check(Input.mouse_mode == Input.MOUSE_MODE_CAPTURED, "hover retains pointer capture")
    var before: Vector3 = player.call("position")
    _key(KEY_S, true)
    await _tick(12)
    _key(KEY_S, false)
    _check(before.distance_to(player.call("position")) > 10.0, "movement continues under container hover")
    _check(await _aim(container), "container reacquired")
    var facing: Vector2 = player.call("facing")
    var motion := InputEventMouseMotion.new()
    motion.relative = Vector2(10, 0)
    Input.parse_input_event(motion)
    Input.flush_buffered_events()
    await _tick()
    _check(absf(player.call("facing").x - facing.x) > 0.001, "mouse look continues under container hover")
    _check(await _aim(container), "container reacquired for activation")
    _key(KEY_E, true)
    await _tick()
    _key(KEY_E, false)
    await _tick(2)
    _check(player.call("is_menu_open"), "short E opens full container menu")
    _check(Input.mouse_mode == Input.MOUSE_MODE_VISIBLE, "activation releases pointer for menu")
    _key(KEY_ESCAPE, true)
    _key(KEY_ESCAPE, false)
    await _tick(3)
    _check(not player.call("is_menu_open"), "Escape closes the container menu")
    _check(Input.mouse_mode == Input.MOUSE_MODE_CAPTURED, "close restores pointer capture")
    before = player.call("position")
    _key(KEY_S, true)
    await _tick(12)
    _key(KEY_S, false)
    _check(before.distance_to(player.call("position")) > 10.0, "close restores movement without stuck input")
    _check(not player.call("is_menu_open"), "continued hover does not reopen menu")
    print("PASS freewalk_hover" if failures == 0 else "FAIL freewalk_hover")
    quit(0 if failures == 0 else 1)
