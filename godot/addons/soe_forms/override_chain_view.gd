extends Tree


func _ready() -> void:
	columns = 4
	column_titles_visible = true
	set_column_title(0, "Plugin")
	set_column_title(1, "Type")
	set_column_title(2, "Editor ID")
	set_column_title(3, "Winner")
	hide_root = true


func show_chain(versions: Array[Dictionary]) -> void:
	clear()
	var root := create_item()
	for version in versions:
		var row := create_item(root)
		row.set_text(0, version.get("plugin_name", ""))
		row.set_text(1, version.get("signature", ""))
		row.set_text(2, version.get("editor_id", ""))
		row.set_text(3, "Yes" if version.get("is_winner", false) else "No")
