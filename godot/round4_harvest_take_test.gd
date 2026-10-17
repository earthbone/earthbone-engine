extends "res://walkthrough.gd"

## Inventory and harvested geometry in the same Whiterun free walk scene.
var checking := false
var failures := 0

func _initialize() -> void:
    startup_cell = "WhiterunOrigin"
    startup_radius = 0
    var items: Array = JSON.parse_string(OS.get_environment("SOE_R4_HARVEST_TAKE"))
    for row: Dictionary in items:
        if row.name == "FoodGoatMeat":
            startup_position = Vector3(row.position[0], row.position[1], row.position[2]) + Vector3(0, -120, -100)
            break
    _watchdog()
    super._initialize()

func _configure_player() -> void:
    player.set("capture_mouse", false)

func _watchdog() -> void:
    await create_timer(240.0).timeout
    printerr("FAIL round4_harvest_take: watchdog")
    quit(1)

func _process(delta: float) -> bool:
    if loaded and not checking:
        checking = true
        call_deferred("_check_items")
    return super._process(delta)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL round4_harvest_take: ", message)

func _labels(node: Node) -> String:
    var result := ""
    if node is Label:
        result += node.text + "\n"
    elif node is Button:
        result += node.text + "\n"
    for child in node.get_children():
        result += _labels(child)
    return result

func _key(pressed: bool) -> void:
    var event := InputEventKey.new()
    event.keycode = KEY_E
    event.pressed = pressed
    Input.parse_input_event(event)
    Input.flush_buffered_events()

func _take_with_input(row: Dictionary) -> void:
    _check(player.call("queue_player_input", {"action": "move_to", "file": int(row.file), "object": int(row.object), "offset": Vector3(0, -85, -110), "match_rotation": false}), "approach " + row.name)
    _check(player.call("step_player_input"), "apply approach")
    for _frame in range(30):
        await physics_frame
        player.call("advance", 1.0 / 60.0)
    var pose: Dictionary = streamer.call("rendered_reference_pose", row.file, row.object)
    _check(not pose.is_empty(), row.name + " is drawn")
    var target: Vector3 = pose.get("position", Vector3.ZERO) + Vector3(row.center[0], row.center[1], row.center[2])
    var eye_godot := get_root().get_camera_3d().global_position
    var eye := Vector3(eye_godot.x, -eye_godot.z, eye_godot.y) * 69.99125
    var direction := target - eye
    var yaw := atan2(direction.x, direction.y)
    var pitch := atan2(direction.z, Vector2(direction.x, direction.y).length())
    _check(player.call("queue_player_input", {"action": "turn", "rotation": Vector3(-pitch, 0, yaw)}), "aim at " + row.name)
    _check(player.call("step_player_input"), "apply aim")
    player.call("advance", 1.0 / 60.0)
    var prompt: Dictionary = player.call("activation_prompt")
    print("round4_harvest_take crosshair ", prompt, " target=", target, " eye=", eye)
    print("round4_harvest_take collision ", row.name, " ", player.call("reference_collision_bodies", row.file, row.object), " streamed=", streamer.call("reference_collision_bodies", row.file, row.object))
    _check(prompt.get("name", "") == row.display_name, row.name + " is the crosshair target")
    if row.harvest:
        _check(prompt.get("action", "") == ("Steal" if prompt.get("stealing", false) else "Harvest"), "harvest action is localized")
    _key(true)
    player.call("advance", 1.0 / 60.0)
    _key(false)
    player.call("advance", 1.0 / 60.0)

func _check_items() -> void:
    player.set("auto_step", false)
    var items: Array = JSON.parse_string(OS.get_environment("SOE_R4_HARVEST_TAKE"))
    for row: Dictionary in items:
        var before: int = player.call("play_path_item_count", row.base_file, row.base_object)
        if row.input:
            await _take_with_input(row)
        else:
            _check(player.call("activate_reference", row.file, row.object), "activation: " + row.name)
        player.call("advance", 1.0 / 60.0)
        await process_frame
        var after: int = player.call("play_path_item_count", row.base_file, row.base_object)
        _check(after > before, "ingredient or item received: " + row.name)
        if row.harvest:
            _check(player.call("activate_reference", row.file, row.object), "repeated plant activation")
            _check(player.call("play_path_item_count", row.base_file, row.base_object) == after, "harvest cannot repeat")
            var pose: Dictionary = streamer.call("rendered_reference_pose", row.file, row.object)
            _check(not pose.is_empty() and not pose.visible, "normal plant geometry replaced: " + row.name)
        _check(player.call("open_inventory_menu"), "inventory opens")
        await process_frame
        _check(_labels(player).contains(row.produce_name), "inventory menu shows " + row.produce_name)
        player.call("close_menu")
        print("round4_harvest_take ", row.name, " count ", before, " -> ", after)
    print("PASS round4_harvest_take" if failures == 0 else "FAIL round4_harvest_take")
    quit(0 if failures == 0 else 1)
