extends CanvasLayer

## Logging console and F3 verbosity controls share the engine subscriber.
var bridge: Node
var panel: PanelContainer
var filter_edit: LineEdit
var command_edit: LineEdit
var output: RichTextLabel
var prior_mouse_mode := Input.MOUSE_MODE_VISIBLE
var refresh_seconds := 0.0

func _ready() -> void:
	bridge = ClassDB.instantiate("SoeLogBridge")
	add_child(bridge)
	layer = 100
	panel = PanelContainer.new()
	panel.position = Vector2(12, 12)
	panel.size = Vector2(610, 320)
	add_child(panel)
	var box := VBoxContainer.new()
	panel.add_child(box)
	var title := Label.new()
	title.text = "Logging, F3 closes. Verbosity: game::ai=debug,papyrus=trace"
	box.add_child(title)
	filter_edit = LineEdit.new()
	filter_edit.text = bridge.call("log_filter")
	filter_edit.text_submitted.connect(_apply_filter)
	box.add_child(filter_edit)
	command_edit = LineEdit.new()
	command_edit.placeholder_text = "Console: log [verbosity directives], Enter applies"
	command_edit.text_submitted.connect(_console)
	box.add_child(command_edit)
	output = RichTextLabel.new()
	output.bbcode_enabled = false
	output.custom_minimum_size = Vector2(580, 230)
	box.add_child(output)
	panel.hide()

func _input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo and (event.keycode == KEY_F3 or event.keycode == KEY_QUOTELEFT):
		if panel.visible:
			panel.hide()
			Input.mouse_mode = prior_mouse_mode
		else:
			prior_mouse_mode = Input.mouse_mode
			Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
			panel.show()
			filter_edit.text = bridge.call("log_filter")
			if event.keycode == KEY_F3:
				filter_edit.grab_focus()
			else:
				command_edit.grab_focus()
		get_viewport().set_input_as_handled()

func _apply_filter(filter_text: String) -> void:
	if not bridge.call("set_log_filter", filter_text):
		output.text = "Invalid filter. Previous verbosity retained."
	else:
		filter_edit.text = bridge.call("log_filter")

func _console(command: String) -> void:
	output.text = bridge.call("console", command)
	filter_edit.text = bridge.call("log_filter")
	command_edit.clear()

func _process(delta: float) -> void:
	refresh_seconds += delta
	if panel.visible and refresh_seconds >= 1.0:
		refresh_seconds = 0.0
		var now: int = bridge.call("log_time_ms")
		var lines: PackedStringArray = bridge.call("log_window", maxi(0, now - 5000), now)
		output.text = "\n".join(lines)
