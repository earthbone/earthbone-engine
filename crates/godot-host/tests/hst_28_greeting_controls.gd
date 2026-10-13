# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree

var failures := 0

func _initialize() -> void:
    _watchdog()
    call_deferred("_run")

func _watchdog() -> void:
    await create_timer(30.0).timeout
    printerr("FAIL hst_28 greeting controls: watchdog")
    quit(1)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL hst_28 greeting controls: ", message)

func _run() -> void:
    var player := ClassDB.instantiate("PlayerNode") as Node3D
    player.set("capture_mouse", false)
    player.set("auto_step", false)
    get_root().add_child(player)
    await process_frame
    _check(player.call("debug_load_mcm_fixture", OS.get_environment("SOE_HST_28_FIXTURE")), "load synthetic records")
    _check(player.call("open_dialogue_menu", 0, 0x100), "open speaker greeting")
    var state: Dictionary = player.call("dialogue_state")
    _check(state.get("text", "") == "Welcome.", "record greeting is shown")
    _check(state.get("topics", []).size() == 1, "one record topic is eligible")
    var content: Control = player.call("menu_content")
    var buttons: Array = content.find_children("*", "Button", true, false)
    var labels: Array[String] = []
    for button: Button in buttons:
        labels.append(button.text)
    print("hst_28 synthetic greeting buttons: ", labels)
    _check(labels == ["Ask about the road"], "greeting actions are the offered topics")
    var focus: Control = player.get_viewport().gui_get_focus_owner()
    _check(focus is Button and (focus as Button).text == "Ask about the road", "default focus selects a topic")
    if not buttons.is_empty():
        (buttons[0] as Button).emit_signal("pressed")
        state = player.call("dialogue_state")
        _check(state.get("text", "") == "The road leads north.", "default action starts the topic response")
    _check(player.call("close_menu"), "close response")
    _check(player.call("open_dialogue_menu", 0, 0x101), "open goodbye greeting")
    state = player.call("dialogue_state")
    _check(state.get("text", "") == "Goodbye." and state.get("topics", []).is_empty(), "goodbye has no eligible topics")
    content = player.call("menu_content")
    buttons = content.find_children("*", "Button", true, false)
    _check(buttons.size() == 1 and (buttons[0] as Button).text == "Continue", "empty greeting retains continuation")
    if not buttons.is_empty():
        (buttons[0] as Button).emit_signal("pressed")
        _check(not player.call("is_menu_open"), "goodbye continuation closes the conversation")
    player.queue_free()
    await process_frame
    print("PASS hst_28 greeting controls" if failures == 0 else "FAIL hst_28 greeting controls checks=" + str(failures))
    quit(0 if failures == 0 else 1)
