extends SceneTree

## Real native controls must fit even below their unscaled theme minimum sizes.
var failures := 0

func _initialize() -> void:
    create_timer(30.0).timeout.connect(func() -> void:
        printerr("FAIL freewalk_menu_layout: watchdog")
        quit(1))
    call_deferred("_run")

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL freewalk_menu_layout: ", message)

func _run() -> void:
    var player: Node = ClassDB.instantiate("PlayerNode")
    player.set("capture_mouse", false)
    player.set("auto_step", false)
    root.add_child(player)
    await process_frame
    _check(player.call("open_menu", "Dialogue"), "dialogue shell opens")
    var panel := player.get_node("MenuLayer/MenuPanel") as Control
    var subtitle := player.get_node("MenuLayer/DialogueSubtitle") as Label
    var speaker := player.get_node("MenuLayer/DialogueSpeaker") as Label
    subtitle.text = "A greeting that wraps across the subtitle area near the bottom of the view."
    speaker.text = "Speaker"
    var content: Control = player.call("menu_content")
    var scroll := ScrollContainer.new()
    scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
    scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
    var rows := VBoxContainer.new()
    rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    for i in 12:
        var button := Button.new()
        button.text = "A dialogue topic that needs to wrap within the topic list " + str(i)
        button.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
        rows.add_child(button)
    scroll.add_child(rows)
    content.add_child(scroll)
    for size: Vector2i in [Vector2i(32, 18), Vector2i(64, 36), Vector2i(160, 90), Vector2i(320, 180), Vector2i(640, 360), Vector2i(1280, 720), Vector2i(800, 1000), Vector2i(1920, 720)]:
        root.size = size
        await process_frame
        await process_frame
        var actual := root.get_visible_rect().size
        var viewport_rect := Rect2(Vector2.ZERO, actual)
        for control: Control in [panel, subtitle, speaker]:
            var rect := control.get_global_rect()
            _check(viewport_rect.encloses(rect), "rect fits " + str(size) + ": " + str(rect))
            _check(rect.position.y >= actual.y * 0.4, "rect stays in lower viewport " + str(size))
        var topic_size := panel.get_global_rect().size
        _check(topic_size.x <= actual.x * 0.4 and topic_size.y <= actual.y * 0.5, "topics leave speaker view visible " + str(size))
        print("freewalk_menu_layout requested=", size, " viewport=", actual, " topics=", panel.get_global_rect())
    root.size = Vector2i(640, 360)
    await process_frame
    _check(player.call("close_menu"), "dialogue closes")
    _check(player.call("open_menu", "Paused"), "ordinary menu opens after dialogue")
    await process_frame
    await process_frame
    _check(panel.scale == Vector2.ONE and panel.get_global_rect().size == root.get_visible_rect().size, "ordinary menu restores full viewport without dialogue scale")
    player.call("close_menu")
    player.queue_free()
    print("PASS freewalk_menu_layout" if failures == 0 else "FAIL freewalk_menu_layout")
    quit(0 if failures == 0 else 1)
