extends "res://walkthrough.gd"

var console_test_started := false
var failures: Array[String] = []

func _configure_player() -> void:
    root.size = Vector2i(1280, 720)
    player.set("capture_mouse", false)

func _spawn_position(_streamer: Node3D) -> Vector3:
    return Vector3(-125.0, -800.0, 120.0)

func _process(delta: float) -> bool:
    super._process(delta)
    if loaded and not console_test_started:
        console_test_started = true
        call_deferred("_console_test")
    return false

func _check(condition: bool, description: String) -> void:
    if not condition:
        failures.append(description)
        printerr("CONSOLE failure: ", description)

func _key(code: int, unicode_value := 0, physical := 0) -> void:
    var event := InputEventKey.new()
    event.keycode = code
    event.physical_keycode = physical
    event.unicode = unicode_value
    event.pressed = true
    Input.parse_input_event(event)
    Input.flush_buffered_events()
    event = event.duplicate()
    event.pressed = false
    Input.parse_input_event(event)
    Input.flush_buffered_events()

func _console_test() -> void:
    for _frame in range(10):
        await process_frame
    player.set("auto_step", false)
    var before: Vector3 = player.call("position")
    var days_before: float = player.call("gameplay_days")
    var old_mouse := Input.mouse_mode
    var gold_before_text := str(player.call("console_execute", "player.getitemcount f"))
    var gold_before := int(gold_before_text)
    _check(gold_before_text.is_valid_int(), "getitemcount returns an integer")
    var gold_lookup := str(player.call("console_execute", "lookup f"))
    _check(gold_lookup.contains("Gold001") and gold_lookup.contains("0000000F"), "short FormID wins over editor ID F")
    _key(KEY_QUOTELEFT)
    _check(player.call("console_is_open"), "grave opens console")
    var line: LineEdit = player.get_node("ConsoleLayer/Console/Content/Input")
    var output: RichTextLabel = player.get_node("ConsoleLayer/Console/Content/Output")
    _check(line.has_focus(), "input has keyboard focus")
    _check(Input.mouse_mode == Input.MOUSE_MODE_VISIBLE, "pointer released for reference selection")
    _key(KEY_I)
    _check(not player.call("is_menu_open"), "inventory hotkey does not reach world")
    var motion := InputEventMouseMotion.new()
    motion.relative = Vector2(100, 100)
    Input.parse_input_event(motion)
    Input.flush_buffered_events()
    for _step in range(10):
        player.call("advance", 1.0 / 60.0)
    _check(before == player.call("position"), "world movement pauses")
    _check(days_before == player.call("gameplay_days"), "world clock pauses")
    var selected_by_click := ""
    for y in range(1, 5):
        for x in range(1, 6):
            var click := InputEventMouseButton.new()
            click.button_index = MOUSE_BUTTON_LEFT
            click.position = root.get_visible_rect().size * Vector2(float(x) / 6.0, float(y) / 10.0)
            click.pressed = true
            Input.parse_input_event(click)
            Input.flush_buffered_events()
            click = click.duplicate()
            click.pressed = false
            Input.parse_input_event(click)
            Input.flush_buffered_events()
            var picked := str(player.call("console_execute", "getselected"))
            if not picked.begins_with("error:"):
                selected_by_click = picked
                break
        if not selected_by_click.is_empty():
            break
    _check(not selected_by_click.is_empty(), "world click selects a reference")
    if not selected_by_click.is_empty():
        _check(str(player.call("console_execute", "getself")) == selected_by_click, "unprefixed command uses click selection")
        var selected_label: Label = player.get_node("ConsoleLayer/Console/Content/Selected")
        _check(not str(player.call("console_execute", "markfordelete")).contains("error:"), "selected reference accepts MarkForDelete")
        _check(selected_label.text.contains("[D]"), "pending deletion appears in selection label")
    line.clear()
    var command := "player.additem f 7"
    for character in command:
        _key(character.to_upper().unicode_at(0), character.unicode_at(0))
    _check(line.text == command, "typed key events fill line")
    _key(KEY_ENTER)
    _check(line.text.is_empty() and output.text.contains("> " + command), "Enter submits to shared Console")
    _key(KEY_UP)
    _check(line.text == command, "Up recalls history")
    _key(KEY_DOWN)
    _check(line.text.is_empty(), "Down restores draft")
    var gold_after_text := str(player.call("console_execute", "player.getitemcount f"))
    var gold_after := int(gold_after_text)
    _check(gold_after_text.is_valid_int(), "updated getitemcount returns an integer")
    _check(gold_after == gold_before + 7, "typed command adds seven gold")
    var bad := str(player.call("console_execute", "thiscommanddoesnotexist"))
    _check(bad.begins_with("error:") and output.text.contains(bad), "errors print in overlay")
    _key(KEY_ESCAPE)
    _check(not player.call("console_is_open"), "Escape closes")
    _check(Input.mouse_mode == old_mouse, "previous mouse mode restored")
    _key(KEY_QUOTELEFT)
    _key(KEY_QUOTELEFT)
    _check(not player.call("console_is_open"), "grave closes")
    _key(KEY_NONE, 126)
    _check(player.call("console_is_open"), "tilde opens")
    _key(KEY_NONE, 96)
    _check(not player.call("console_is_open"), "grave Unicode closes")
    _key(KEY_NONE, 0, KEY_QUOTELEFT)
    _check(player.call("console_is_open"), "physical grave supports other layouts")
    _key(KEY_ESCAPE)
    player.call("console_execute", "fov 90")
    _check(is_equal_approx(root.get_camera_3d().fov, 90.0), "fov updates the live camera")
    var player_before: Vector3 = player.call("position")
    var camera_before := root.get_camera_3d().global_position
    player.call("console_execute", "tfc 1")
    var forward := InputEventKey.new()
    forward.keycode = KEY_W
    forward.pressed = true
    Input.parse_input_event(forward)
    Input.flush_buffered_events()
    player.call("advance", 1.0 / 60.0)
    forward = forward.duplicate()
    forward.pressed = false
    Input.parse_input_event(forward)
    Input.flush_buffered_events()
    _check(root.get_camera_3d().global_position.distance_to(camera_before) > 0.01, "free camera moves")
    _check(player_before == player.call("position"), "free camera leaves player reference in place")
    player.call("console_execute", "tfc")
    player.call("console_execute", "prid 00000014")
    var selection: Label = player.get_node("ConsoleLayer/Console/Content/Selected")
    _check(selection.text.contains("00000014"), "prid updates vanilla runtime FormID label")
    for operation in [["ts", "sky"], ["teofis", "imagespace"], ["s1st", "first_person"]]:
        var features_before: Dictionary = player.call("console_features")
        var result := str(player.call("console_execute", operation[0]))
        var features_after: Dictionary = player.call("console_features")
        _check(not result.contains("error:") and features_before[operation[1]] != features_after[operation[1]], operation[0] + " changes host presentation")
        player.call("console_execute", operation[0])
        _check(player.call("console_features")[operation[1]] == features_before[operation[1]], operation[0] + " restores host presentation")
    var prior_debug := root.debug_draw
    player.call("console_execute", "twf")
    _check(root.debug_draw == Viewport.DEBUG_DRAW_WIREFRAME, "twf changes actual viewport draw mode")
    player.call("console_execute", "twf")
    _check(root.debug_draw == prior_debug, "twf restores viewport draw mode")
    for operation in ["tg", "tt", "tws", "tb", "tll"]:
        _check(not str(player.call("console_execute", operation)).contains("error:"), operation + " reaches live streamer")
        _check(not str(player.call("console_execute", operation)).contains("error:"), operation + " restores streamer presentation")
    var prior_third := bool(player.call("is_third_person"))
    var actor_angle := float(str(player.call("console_execute", "player.getangle z")))
    player.call("console_execute", "animcam")
    _check(player.call("is_third_person"), "animcam selects third person")
    player.call("set_facing", deg_to_rad(actor_angle) + 0.5, 0.0)
    player.call("advance", 1.0 / 60.0)
    _check(absf(float(str(player.call("console_execute", "player.getangle z"))) - actor_angle) < 0.001, "animcam leaves actor facing unchanged")
    player.call("console_execute", "animcam")
    _check(bool(player.call("is_third_person")) == prior_third, "animcam restores prior view")
    print("CONSOLE_OVERLAY_RESULT ", "PASS" if failures.is_empty() else "FAIL", " failures=", failures)
    quit(0 if failures.is_empty() else 1)
