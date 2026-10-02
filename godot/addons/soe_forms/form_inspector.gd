@tool
extends EditorInspectorPlugin


func _can_handle(object: Object) -> bool:
	return object.get_class() == "SoeFormResource"


func _parse_begin(object: Object) -> void:
	if _can_handle(object):
		add_custom_control(winning_source_control(object))


func winning_source_control(object: Object) -> Control:
	var row := HBoxContainer.new()
	row.name = "SoeWinningSource"
	var title := Label.new()
	title.text = "Winning source:"
	row.add_child(title)
	var source := Label.new()
	source.name = "SoeWinningSourceValue"
	source.text = object.get("winning_plugin")
	source.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(source)
	return row
