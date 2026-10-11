extends "res://walkthrough.gd"

## HST-28 requires Hulda's drawn rig, a condition-selected greeting and topics,
## voice playback, nonzero head mouth weights and a return to play on closing.
var failures := 0
var checking := false

func _initialize() -> void:
    _watchdog()
    super._initialize()

func _watchdog() -> void:
    await create_timer(240.0).timeout
    printerr("FAIL hst_28: watchdog")
    quit(1)

func _configure_player() -> void:
    player.set("capture_mouse", false)
    player.set("auto_step", false)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL hst_28: ", message)

func _process(delta: float) -> bool:
    if loaded and not checking:
        checking = true
        call_deferred("_run_hst_28")
    return super._process(delta)

func _key(pressed: bool) -> void:
    var event := InputEventKey.new()
    event.keycode = KEY_E
    event.physical_keycode = KEY_E
    event.pressed = pressed
    Input.parse_input_event(event)
    Input.flush_buffered_events()

func _run_hst_28() -> void:
    system_overlay.queue_free()
    await process_frame
    var speaker_file := int(OS.get_environment("SOE_HST_28_SPEAKER_FILE"))
    var speaker_object := int(OS.get_environment("SOE_HST_28_SPEAKER_OBJECT"))
    var hulda: Dictionary = {}
    var deadline := Time.get_ticks_msec() + 90_000
    while Time.get_ticks_msec() < deadline:
        await process_frame
        for visual: Dictionary in player.call("npc_visuals"):
            if visual.file == speaker_file and visual.object == speaker_object and visual.shapes > 0:
                hulda = visual
                break
        if not hulda.is_empty(): break
    _check(not hulda.is_empty(), "Hulda is drawn in the Bannered Mare")
    if hulda.is_empty():
        print("hst_28 actors: ", player.call("npc_visual_counts"), player.call("npc_visuals"))
        quit(1)
        return
    var actor := instance_from_id(int(hulda.node_id)) as Node
    var acquired := false
    for angle in range(12):
        var theta := angle * TAU / 12.0
        var offset := Vector3(sin(theta), cos(theta), 0) * 90.0
        _check(player.call("queue_player_input", {
            "action": "move_to", "file": hulda.file, "object": hulda.object,
            "offset": offset, "match_rotation": false
        }), "move near Hulda")
        player.call("step_player_input")
        player.call("set_facing", theta + PI, -0.25)
        player.call("advance", 1.0 / 60.0)
        await physics_frame
        player.call("advance", 1.0 / 60.0)
        var focused: Dictionary = player.call("carry_status")
        if focused.get("crosshair_file", -1) == hulda.file and focused.get("crosshair_object", -1) == hulda.object:
            acquired = true
            break
    _check(acquired, "crosshair ray targets Hulda")
    if not acquired:
        quit(1)
        return
    await process_frame
    await process_frame
    var prompt := player.get_node("HudLayer/Hud/ActivationPrompt") as Label
    print("hst_28 before E: ", player.call("dialogue_state"), " prompt=", prompt.text, " visible=", prompt.visible)
    _check(prompt.visible and prompt.text == "Talk\nHulda", "crosshair shows Talk and Hulda's name")
    _key(true)
    player.call("advance", 1.0 / 60.0)
    await physics_frame
    _key(false)
    player.call("advance", 1.0 / 60.0)
    _check(player.call("is_menu_open"), "E opens dialogue")
    var state: Dictionary = player.call("dialogue_state")
    print("hst_28 greeting: ", state)
    _check(state.get("speaker_object", -1) == hulda.object, "conversation belongs to Hulda")
    _check(not str(state.get("text", "")).is_empty(), "Hulda greeting selected")
    _check(not state.get("topics", []).is_empty(), "greeting opens alongside eligible topics")
    _check(state.get("voice_slot", -1) >= 0, "greeting's installed voice starts")
    var greeting_info := int(state.get("info_object", -1))
    var weights_seen := false
    deadline = Time.get_ticks_msec() + 5_000
    while Time.get_ticks_msec() < deadline:
        await process_frame
        var mouth: Dictionary = actor.call("dialogue_mouth_state")
        for weight: float in mouth.values():
            if absf(weight) > 0.001: weights_seen = true
        if weights_seen: break
    _check(weights_seen, "voice clock drives Hulda head mouth morphs")
    print("hst_28 lips: ", actor.call("dialogue_mouth_state"))
    state = player.call("dialogue_state")
    var topics: Array = state.get("topics", [])
    _check(not topics.is_empty(), "conditions offer at least one Hulda topic")
    var content: Control = player.call("menu_content")
    var topic_button: Button
    if not topics.is_empty():
        for node: Node in content.find_children("*", "Button", true, false):
            var button := node as Button
            if button.text == topics[0].label:
                topic_button = button
                break
    _check(topic_button != null, "offered topic is a menu button")
    if topic_button != null:
        topic_button.emit_signal("pressed")
        state = player.call("dialogue_state")
        _check(int(state.get("info_object", -1)) != greeting_info, "topic button selects a response INFO")
        _check(not str(state.get("text", "")).is_empty(), "topic button starts its response")
        _check(state.get("voice_slot", -1) >= 0, "response's installed voice starts")
        var has_line := false
        for node: Node in content.find_children("*", "Label", true, false):
            var label := node as Label
            if label.text == state.get("text", ""): has_line = true
        _check(has_line, "menu shows the game-selected response line")
    _check(player.call("close_menu"), "close returns to play")
    _check(not player.call("is_menu_open") and player.call("dialogue_state").is_empty(), "menu and game conversation close together")
    await process_frame
    await process_frame
    for weight: float in actor.call("dialogue_mouth_state").values():
        _check(weight == 0.0, "closing resets mouth weights")
    var before: float = player.call("gameplay_days")
    player.call("advance", 1.0)
    _check(player.call("gameplay_days") > before, "game clock resumes")
    print("PASS hst_28" if failures == 0 else "FAIL hst_28 checks=" + str(failures))
    quit(0 if failures == 0 else 1)
