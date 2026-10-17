extends "res://walkthrough.gd"

var failures: Array[String] = []

func _configure_player() -> void:
    root.size = Vector2i(960, 540)
    player.set("capture_mouse", false)

func _spawn_position(_source: Node3D) -> Vector3:
    if startup_cell == "WhiterunExterior04":
        return Vector3(19158.225, -7419.267, -3516.4268)
    return Vector3(-125, -800, 120)

func _key(code: int, ctrl := false, shift := false, unicode_value := 0) -> void:
    var event := InputEventKey.new()
    event.keycode = code
    event.ctrl_pressed = ctrl
    event.shift_pressed = shift
    event.unicode = unicode_value
    event.pressed = true
    Input.parse_input_event(event)
    Input.flush_buffered_events()
    event = event.duplicate()
    event.pressed = false
    Input.parse_input_event(event)
    Input.flush_buffered_events()

func _mouse(point: Vector2, button: int, pressed: bool) -> void:
    var event := InputEventMouseButton.new()
    event.position = point
    event.global_position = point
    event.button_index = button
    event.pressed = pressed
    Input.parse_input_event(event)
    Input.flush_buffered_events()

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures.append(message)
        printerr("R4_TEXT failure: ", message)

func _start() -> void:
    if OS.get_environment("SOE_R4_TEXT_CELL") == "WhiterunExterior04":
        startup_cell = "WhiterunExterior04"
        startup_radius = 0
    await super._start()
    if not loaded:
        return
    player.set("auto_step", false)
    player.call("console_set_open", true)
    await process_frame
    var line: LineEdit = player.get_node("ConsoleLayer/Console/Content/Input")
    var output: RichTextLabel = player.get_node("ConsoleLayer/Console/Content/Output")
    var clipboard_before := DisplayServer.clipboard_get()
    var primary_before := DisplayServer.clipboard_get_primary()
    DisplayServer.clipboard_set("player.getpos x")
    _key(KEY_V, true)
    _check(line.text == "player.getpos x", "Ctrl+V pastes clipboard")
    _key(KEY_A, true)
    _check(line.get_selected_text() == "player.getpos x", "Ctrl+A selects input")
    DisplayServer.clipboard_set("sentinel")
    _key(KEY_C, true)
    _check(DisplayServer.clipboard_get() == "player.getpos x", "Ctrl+C copies input selection")
    _key(KEY_HOME)
    _check(line.caret_column == 0, "Home moves to start")
    _key(KEY_END)
    _check(line.caret_column == line.text.length(), "End moves to end")
    _key(KEY_LEFT, true, true)
    _check(line.get_selected_text() == "x", "Ctrl+Shift+Left selects word")
    _key(KEY_HOME)
    _key(KEY_RIGHT, true)
    _check(line.caret_column == 6, "Ctrl+Right moves over player")
    _key(KEY_HOME)
    _key(KEY_RIGHT, false, true)
    _check(line.get_selected_text() == "p", "Shift+Right selects one character")
    _key(KEY_END, false, true)
    _check(line.get_selected_text() == line.text, "Shift+End extends selection")
    _key(KEY_LEFT)
    _check(not line.has_selection(), "Left clears selection")
    line.clear()
    DisplayServer.clipboard_set_primary("prid 00000014")
    var input_point := line.get_global_rect().get_center()
    _mouse(input_point, MOUSE_BUTTON_MIDDLE, true)
    _mouse(input_point, MOUSE_BUTTON_MIDDLE, false)
    _check(line.text == "prid 00000014", "middle click pastes Linux primary selection")
    _key(KEY_ENTER)
    player.call("console_execute", "player.getpos x")
    player.call("console_execute", "help Gold001 4")
    await process_frame
    await process_frame
    var rect := output.get_global_rect()
    var hover := InputEventMouseMotion.new()
    var line_height := output.get_theme_font("normal_font").get_height(output.get_theme_font_size("normal_font_size"))
    hover.position = rect.position + Vector2(1, output.get_line_offset(0) + line_height * 0.5)
    hover.global_position = hover.position
    Input.parse_input_event(hover)
    Input.flush_buffered_events()
    _mouse(hover.position, MOUSE_BUTTON_LEFT, true)
    await process_frame
    var drag := InputEventMouseMotion.new()
    drag.position = Vector2(rect.end.x - 25, rect.position.y + output.get_line_offset(output.get_line_count() - 1) + line_height * 0.5)
    drag.global_position = drag.position
    drag.relative = drag.position - (rect.position + Vector2(5, 5))
    drag.button_mask = MOUSE_BUTTON_MASK_LEFT
    Input.parse_input_event(drag)
    Input.flush_buffered_events()
    await process_frame
    _mouse(drag.position, MOUSE_BUTTON_LEFT, false)
    await process_frame
    var selected := output.get_selected_text()
    print("R4_TEXT_SELECTION enabled=", output.selection_enabled, " focus=", output.has_focus(), " rect=", rect, " text=", output.text, " selected=", selected)
    _check(selected.contains("\n") and selected.contains("00000014") and selected.contains("Gold001"), "mouse selects printed results across lines")
    DisplayServer.clipboard_set("sentinel")
    _key(KEY_C, true)
    _check(not selected.is_empty() and DisplayServer.clipboard_get() == selected, "Ctrl+C copies selected output")
    for index in range(24):
        player.call("console_execute", "player.getpos x")
    await process_frame
    await process_frame
    var bar := output.get_v_scroll_bar()
    var before := bar.value
    _key(KEY_PAGEUP)
    _check(bar.value < before, "Page Up scrolls output")
    var up := bar.value
    _key(KEY_PAGEDOWN)
    _check(bar.value > up, "Page Down scrolls output")
    line.grab_focus()
    line.text = "getp"
    line.caret_column = 4
    _key(KEY_TAB)
    _check(line.text.begins_with("getp") and line.text != "getp", "Tab completion remains available")
    _key(KEY_ESCAPE)
    _check(not player.call("console_is_open"), "Escape closes after output selection")
    DisplayServer.clipboard_set(clipboard_before)
    DisplayServer.clipboard_set_primary(primary_before)
    print("R4_TEXT_RESULT ", "PASS" if failures.is_empty() else "FAIL", " failures=", failures)
    quit(0 if failures.is_empty() else 1)
