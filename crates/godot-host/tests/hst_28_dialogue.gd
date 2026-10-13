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
    var bank := player.get_node("SoundBank")
    _check(bank.call("play_music_track", OS.get_environment("SOE_HST_28_MUSIC_PATH"), 0.0, 0.5, false), "audible installed music queued for category check")
    deadline = Time.get_ticks_msec() + 10_000
    while int(bank.call("music_playing_count")) == 0 and Time.get_ticks_msec() < deadline:
        await process_frame
    _check(int(bank.call("music_playing_count")) > 0, "installed music starts before dialogue")
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
    var slots: Array[AudioStreamPlayer3D] = []
    for child: Node in bank.get_children():
        if child is AudioStreamPlayer3D: slots.append(child)
    var voice: AudioStreamPlayer3D
    var voice_slot := int(state.get("voice_slot", -1))
    if voice_slot >= 0 and voice_slot < slots.size(): voice = slots[voice_slot]
    _check(voice != null and voice.playing, "dialogue owns its reported active host voice slot")
    if voice != null:
        var expected_db := 20.0 * log(0.9 * 0.8 * 0.9) / log(10.0)
        var gain_db := voice.volume_db
        var bus := AudioServer.get_bus_index(voice.bus)
        while bus >= 0:
            gain_db += AudioServer.get_bus_volume_db(bus)
            if bus == 0: break
            bus = AudioServer.get_bus_index(AudioServer.get_bus_send(bus))
        _check(absf(gain_db - expected_db) <= 1.0, "voice level within 1 dB of vanilla Voice category")
        _check(voice.attenuation_model == AudioStreamPlayer3D.ATTENUATION_DISABLED and voice.max_distance == 0.0, "host does not add generic distance loss")
        _check(voice.attenuation_filter_db == 0.0, "category volume does not enable a distance filter")
        print("hst_28 voice category_db=", expected_db, " playback_db=", gain_db)
    var music_checked := false
    for child: Node in bank.get_children():
        if child is AudioStreamPlayer and str(child.name).begins_with("MusicPlayer") and child.playing:
            var expected_music_db := 20.0 * log(0.7 * 0.5 * 0.9) / log(10.0)
            _check(child.volume_db <= expected_music_db + 1.0, "music fade stays below its authored category level")
            music_checked = true
            print("hst_28 music category_db=", expected_music_db, " playback_db=", child.volume_db)
    _check(music_checked, "music category checked on active walkthrough playback")
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
    await _check_dialogue_rects()
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
        var subtitle := player.get_node("MenuLayer/DialogueSubtitle") as Label
        _check(subtitle.visible and subtitle.text == state.get("text", ""), "menu shows the game-selected response line")
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

func _check_dialogue_rects() -> void:
    var panel := player.get_node("MenuLayer/MenuPanel") as Control
    var subtitle := player.get_node("MenuLayer/DialogueSubtitle") as Control
    var speaker := player.get_node("MenuLayer/DialogueSpeaker") as Control
    var original: Vector2i = get_root().size
    for size: Vector2i in [Vector2i(320, 180), Vector2i(640, 360), Vector2i(1280, 720), Vector2i(1024, 576), Vector2i(800, 1000), Vector2i(1920, 720)]:
        get_root().size = size
        await process_frame
        await process_frame
        var actual := get_root().get_visible_rect().size
        var viewport_rect := Rect2(Vector2.ZERO, actual)
        for control: Control in [panel, subtitle, speaker]:
            var rect := control.get_global_rect()
            _check(viewport_rect.encloses(rect), "dialogue rect fits viewport " + str(size) + ": " + str(rect))
            _check(rect.position.y >= actual.y * 0.4, "dialogue rect stays in lower viewport " + str(size))
        var topic_size := panel.get_global_rect().size
        _check(topic_size.x <= actual.x * 0.4 and topic_size.y <= actual.y * 0.5, "dialogue topics leave speaker view visible")
        print("hst_28 dialogue viewport=", size, " topics=", panel.get_global_rect(), " subtitle=", subtitle.get_global_rect())
    get_root().size = original
    await process_frame
