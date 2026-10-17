extends "res://walkthrough.gd"

var failures := 0

func _configure_player() -> void:
    player.set("capture_mouse", false)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("R4_COMPLETION FAIL: ", message)

func _tab() -> void:
    var event := InputEventKey.new()
    event.keycode = KEY_TAB
    event.pressed = true
    Input.parse_input_event(event)
    Input.flush_buffered_events()
    event = event.duplicate()
    event.pressed = false
    Input.parse_input_event(event)
    Input.flush_buffered_events()

func _start() -> void:
    await super._start()
    if not loaded:
        return
    player.set("auto_step", false)
    player.call("console_set_open", true)
    var line: LineEdit = player.get_node("ConsoleLayer/Console/Content/Input")
    line.text = "player.addi"
    line.caret_column = line.text.length()
    _tab()
    _check(line.text == "player.additem", "Tab completes command from shared table")
    _check(line.has_focus(), "Tab retains console input focus")
    line.text = "player.additem gold0 7"
    line.caret_column = 20
    _tab()
    _check(line.text == "player.additem Gold001 7", "base editor ID completion preserves following argument")
    line.text = "player.additem f"
    line.caret_column = line.text.length()
    _tab()
    _check(line.text == "player.additem 0000000F", "short numeric FormID completion")
    var references: Array = player.call("gameplay_named_references")
    _check(not references.is_empty(), "free walk has named reference arguments")
    if not references.is_empty():
        var editor: String = references[0].editor_id
        line.text = "prid " + editor
        line.caret_column = line.text.length()
        _tab()
        _check(line.text == "prid " + editor, "reference editor ID completion")
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4_COMPLETION failures=", failures)
    quit(0 if failures == 0 else 1)
