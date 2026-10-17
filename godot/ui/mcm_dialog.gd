extends ConfirmationDialog

## Native controls for the original config script's color and remap protocol.
var host: Node
var option_id := -1
var picker: ColorPicker
var default_color := -1
var keymap := false

func configure(owner_node: Node, id: int, caption: String, remap: bool, start: int, default_value: int) -> void:
	host = owner_node
	option_id = id
	title = caption
	keymap = remap
	default_color = default_value
	var content := VBoxContainer.new()
	add_child(content)
	if keymap:
		var prompt := Label.new()
		prompt.text = "Press a key, mouse button or gamepad control."
		content.add_child(prompt)
		get_ok_button().hide()
	else:
		picker = ColorPicker.new()
		picker.name = "ColorPicker"
		picker.edit_alpha = false
		picker.sliders_visible = false
		picker.presets_visible = false
		picker.color_modes_visible = false
		picker.color = _color(start)
		content.add_child(picker)
		var reset := Button.new()
		reset.name = "Default"
		reset.text = "Default"
		reset.disabled = default_color < 0
		reset.pressed.connect(func(): picker.color = _color(default_color))
		content.add_child(reset)
	confirmed.connect(_accept)
	canceled.connect(func(): host.call("mcm_dialog_cancel", option_id))
	close_requested.connect(func(): host.call("mcm_dialog_cancel", option_id))
	popup_centered()

func _color(value: int) -> Color:
	return Color8((value >> 16) & 255, (value >> 8) & 255, value & 255)

func _accept() -> void:
	if not keymap:
		var color := picker.color
		var value := (roundi(color.r * 255.0) << 16) | (roundi(color.g * 255.0) << 8) | roundi(color.b * 255.0)
		host.call("mcm_dialog_accept", option_id, value)

func _input(event: InputEvent) -> void:
	var viewport := get_viewport()
	if visible and is_instance_valid(host) and host.call("mcm_dialog_event", event):
		viewport.set_input_as_handled()
