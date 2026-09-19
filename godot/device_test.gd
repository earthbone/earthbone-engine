extends SceneTree

## Injected device events exercise PlayerNode without physical peripherals.
var player: Node3D
var failures := 0
const STEP := 1.0 / 60.0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _send(event: InputEvent) -> void:
	Input.parse_input_event(event)
	Input.flush_buffered_events()

func _button(device: int, button: JoyButton, down: bool) -> void:
	var event := InputEventJoypadButton.new()
	event.device = device
	event.button_index = button
	event.pressed = down
	_send(event)

func _axis(device: int, axis: JoyAxis, value: float) -> void:
	var event := InputEventJoypadMotion.new()
	event.device = device
	event.axis = axis
	event.axis_value = value
	_send(event)

func _mouse(button: MouseButton, down: bool) -> void:
	var event := InputEventMouseButton.new()
	event.button_index = button
	event.pressed = down
	_send(event)

func _key(code: Key, down: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = code
	event.pressed = down
	_send(event)

func _step() -> void:
	player.call("advance", STEP)

func _pov() -> bool:
	return player.call("is_third_person")

func _run() -> void:
	if not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: PlayerNode is not registered")
		quit(1)
		return
	player = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	player.call("apply_control_map", "Toggle POV\t0x21\t0x0+0x1,0x8\t0x100+0x200,0x1000\t1\t1\t1\n")
	var events := InputMap.action_get_events("Toggle POV")
	_check(events.size() == 5, "InputMap did not publish all device alternatives")
	_button(0, JOY_BUTTON_LEFT_SHOULDER, true)
	_button(1, JOY_BUTTON_RIGHT_SHOULDER, true)
	_step()
	_check(not _pov(), "two pads completed one chord")
	_button(0, JOY_BUTTON_RIGHT_SHOULDER, true)
	_button(0, JOY_BUTTON_RIGHT_SHOULDER, false)
	_step()
	_check(_pov(), "short pad chord was lost")
	_step()
	_check(_pov(), "short pad chord fired twice")
	_button(0, JOY_BUTTON_LEFT_SHOULDER, false)
	_button(1, JOY_BUTTON_RIGHT_SHOULDER, false)
	_button(1, JOY_BUTTON_A, true)
	_step()
	_check(not _pov(), "alternate pad button did not fire")
	_step()
	_check(not _pov(), "held pad button fired twice")
	Input.joy_connection_changed.emit(1, false)
	_step()
	_button(1, JOY_BUTTON_A, true)
	_step()
	_check(_pov(), "disconnect left the pad button held")
	_button(1, JOY_BUTTON_A, false)
	_step()
	_mouse(MOUSE_BUTTON_LEFT, true)
	_step()
	_check(_pov(), "partial mouse chord fired")
	_mouse(MOUSE_BUTTON_RIGHT, true)
	_mouse(MOUSE_BUTTON_RIGHT, false)
	_mouse(MOUSE_BUTTON_LEFT, false)
	_step()
	_check(not _pov(), "short mouse chord was lost")
	_mouse(MOUSE_BUTTON_WHEEL_UP, true)
	_step()
	_check(_pov(), "wheel did not fire")
	_step()
	_check(_pov(), "wheel repeated without another event")
	_mouse(MOUSE_BUTTON_WHEEL_UP, true)
	_step()
	_check(not _pov(), "second wheel pulse was lost")
	_key(KEY_F, true)
	_key(KEY_F, false)
	_step()
	_check(_pov(), "keyboard tap with a pad binding was lost")
	_step()
	_check(_pov(), "keyboard tap fired twice")
	_key(KEY_F, true)
	_step()
	_check(not _pov(), "second keyboard press did not fire")
	_button(0, JOY_BUTTON_A, true)
	_step()
	_check(not _pov(), "pad repressed a keyboard-held action")
	_key(KEY_F, false)
	_button(0, JOY_BUTTON_A, false)
	_step()

	# Rebinding a currently held trigger must not synthesize another press.
	_axis(0, JOY_AXIS_TRIGGER_LEFT, 0.9)
	player.call("apply_control_map", "Toggle POV\t0xff\t0xff\t0x9\t1\t1\t1\n")
	_step()
	_check(not _pov(), "rebinding a held trigger created a press")
	_axis(0, JOY_AXIS_TRIGGER_LEFT, 0.0)
	_step()
	_axis(0, JOY_AXIS_TRIGGER_LEFT, 0.4)
	_step()
	_check(not _pov(), "trigger fired below threshold")
	_axis(0, JOY_AXIS_TRIGGER_LEFT, 0.9)
	_step()
	_check(_pov(), "trigger did not fire above threshold")
	_step()
	_check(_pov(), "held trigger fired twice")
	player.call("apply_control_map", "Toggle POV\t0xff\t0xff\t0xff\t1\t1\t1\n")
	_axis(0, JOY_AXIS_TRIGGER_LEFT, 0.0)
	_axis(0, JOY_AXIS_TRIGGER_LEFT, 0.9)
	_step()
	_check(_pov(), "removed trigger binding still fired")

	player.call("apply_control_map", "Move\t0xff\t0xff\t0xb\t0\t0\t0\nLook\t0xff\t0xff\t0xc\t0\t0\t0\n")
	player.call("set_facing", 0.0, 0.0)
	_axis(0, JOY_AXIS_RIGHT_X, 0.1)
	_axis(0, JOY_AXIS_RIGHT_Y, -0.1)
	_step()
	_check(player.call("facing") == Vector2.ZERO, "stick drift passed the dead zone")
	_axis(0, JOY_AXIS_RIGHT_X, 0.6)
	_axis(0, JOY_AXIS_RIGHT_Y, -0.6)
	for i in range(60):
		_step()
	var sixty: Vector2 = player.call("facing")
	_check(sixty.x > 0.0 and sixty.y > 0.0, "stick look has wrong direction")
	player.call("set_facing", 0.0, 0.0)
	for i in range(30):
		player.call("advance", 1.0 / 30.0)
	var thirty: Vector2 = player.call("facing")
	_check(sixty.distance_to(thirty) < 0.0001, "stick look depends on frame rate")
	Input.joy_connection_changed.emit(0, false)
	_step()
	_check(player.call("facing") == thirty, "disconnected stick kept turning")
	_axis(0, JOY_AXIS_RIGHT_X, 0.9)
	get_root().focus_exited.emit()
	_axis(0, JOY_AXIS_RIGHT_X, 1.0)
	_key(KEY_F, true)
	_step()
	_check(player.call("facing") == thirty, "unfocused stick kept turning")
	get_root().focus_entered.emit()
	_step()
	_check(player.call("facing") == thirty, "focus restore retained old stick input")
	_check(_pov(), "focus restore replayed a keyboard press")
	_key(KEY_F, false)
	player.call("apply_control_map", "Toggle POV\t0x21\t0xff\t0x1000\t1\t1\t1\nLook\t0xff\t0xff\t0xc\t0\t0\t0\n")
	player.set("auto_step", true)
	_axis(0, JOY_AXIS_RIGHT_X, 0.6)
	_button(0, JOY_BUTTON_A, true)
	await physics_frame
	await process_frame
	_check(not _pov(), "automatic physics did not consume pad press")
	var auto_facing: Vector2 = player.call("facing")
	await physics_frame
	await process_frame
	_check(not _pov(), "automatic physics repeated held pad press")
	_check(player.call("facing").x > auto_facing.x, "automatic process did not sample stick look")
	_button(0, JOY_BUTTON_A, false)
	player.set("auto_step", false)
	player.call("apply_control_map", "")
	await process_frame
	await process_frame
	await process_frame
	Input.action_press("Toggle POV")
	Input.action_release("Toggle POV")
	_step()
	var first_tap := _pov()
	# Manual stepping may be idle long after a latch expires.
	for i in range(4):
		await process_frame
	Input.action_press("Toggle POV")
	Input.action_release("Toggle POV")
	_step()
	_check(_pov() != first_tap, "expired latch swallowed a later synthetic tap")
	player.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " device runtime")
	quit(0 if failures == 0 else 1)
