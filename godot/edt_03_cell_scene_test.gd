extends SceneTree


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var data_path := OS.get_environment("SKYRIM_DATA")
	assert(not data_path.is_empty(), "SKYRIM_DATA must point to the Skyrim Data directory")
	var streamer: Variant = ClassDB.instantiate("CellStreamerNode")
	assert(streamer is Node, "CellStreamerNode is not registered")
	root.add_child(streamer)
	assert(streamer.load_data(data_path), "the installed game data did not load")
	var rows: Array = streamer.editor_cell_tree()
	assert(not rows.is_empty(), "the editor cell tree is empty")
	var selected: Dictionary = {}
	for row: Dictionary in rows:
		if row.interior and row.references > 50 and row.name == "WhiterunBanneredMare":
			selected = row
			break
	if selected.is_empty():
		for row: Dictionary in rows:
			if row.interior and row.references > 50:
				selected = row
				break
	assert(not selected.is_empty(), "no populated interior cell was available")
	var replacement: Dictionary = {}
	for row: Dictionary in rows:
		if row.interior and row.references > 50 and row.id != selected.id:
			replacement = row
			break
	assert(not replacement.is_empty(), "no second populated interior cell was available")

	assert(streamer.open_cell_id(selected.id) >= 0, "the selected cell did not open")
	assert(streamer.last_stats().references > 0, "the opened cell has no placed references")
	assert(streamer.instance_count() > 0, "the shared host scene path created no render instances")
	var first_scene: Dictionary = streamer.cell_instances()
	assert(first_scene.size() == 1 and first_scene.has(selected.id), "the scene view does not contain the selected cell")

	assert(streamer.open_cell_id(replacement.id) >= 0, "the replacement cell did not open")
	assert(streamer.last_stats().references > 0, "the replacement cell has no placed references")
	assert(streamer.instance_count() > 0, "the replacement scene has no render instances")
	var replacement_scene: Dictionary = streamer.cell_instances()
	assert(replacement_scene.size() == 1, "replacing the selected cell left other cells in the scene")
	assert(replacement_scene.has(replacement.id), "the scene view does not contain the replacement cell")
	assert(not replacement_scene.has(selected.id), "the previously selected cell remains in the scene")
	streamer.free()
	quit(0)
