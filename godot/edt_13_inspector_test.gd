@tool
extends RefCounted


static func edt_13_inspector_displays_winning_source(inspector: EditorInspectorPlugin) -> bool:
	var form = ClassDB.instantiate("SoeFormResource")
	form.set("winning_plugin", "Base.esm")
	if not inspector._can_handle(form):
		push_error("Form inspector did not claim SoeFormResource")
		return false
	EditorInterface.edit_resource(form)
	await Engine.get_main_loop().process_frame
	var source := _find_named(EditorInterface.get_inspector(), "SoeWinningSourceValue") as Label
	if source == null or source.text != "Base.esm":
		push_error("Godot inspector did not display winning source Base.esm")
		return false
	return true


static func _find_named(node: Node, target_name: String) -> Node:
	if node.name == target_name:
		return node
	for child in node.get_children():
		var found := _find_named(child, target_name)
		if found != null:
			return found
	return null
