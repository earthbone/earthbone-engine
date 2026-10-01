@tool
extends EditorPlugin

const CELL_STREAMER_CLASS := "CellStreamerNode"

var dock: VBoxContainer
var data_path: LineEdit
var tree: Tree
var viewport: SubViewport
var streamer
var camera: Camera3D


func _enter_tree() -> void:
	dock = VBoxContainer.new()
	dock.name = "Skyrim Cells"
	dock.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	dock.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var path_row := HBoxContainer.new()
	data_path = LineEdit.new()
	data_path.placeholder_text = "Skyrim Data directory"
	data_path.text = OS.get_environment("SKYRIM_DATA")
	data_path.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	path_row.add_child(data_path)
	var load_button := Button.new()
	load_button.text = "Load"
	load_button.pressed.connect(_load_data)
	path_row.add_child(load_button)
	dock.add_child(path_row)

	var split := HSplitContainer.new()
	split.size_flags_vertical = Control.SIZE_EXPAND_FILL
	dock.add_child(split)
	tree = Tree.new()
	tree.hide_root = true
	tree.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	tree.item_selected.connect(_open_selected_cell)
	split.add_child(tree)
	var view_container := SubViewportContainer.new()
	view_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	view_container.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view_container.stretch = true
	viewport = SubViewport.new()
	viewport.size = Vector2i(960, 640)
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	view_container.add_child(viewport)
	split.add_child(view_container)

	var streamer_object: Object = ClassDB.instantiate(CELL_STREAMER_CLASS)
	if streamer_object is Node3D:
		streamer = streamer_object
		viewport.add_child(streamer)
	camera = Camera3D.new()
	camera.current = true
	viewport.add_child(camera)
	add_control_to_dock(DOCK_SLOT_LEFT_UL, dock)


func _exit_tree() -> void:
	remove_control_from_docks(dock)
	if is_instance_valid(dock):
		dock.queue_free()


func _load_data() -> void:
	if not is_instance_valid(streamer) or not streamer.load_data(data_path.text):
		return
	_rebuild_tree()


func _rebuild_tree() -> void:
	tree.clear()
	var root := tree.create_item()
	var interiors := tree.create_item(root)
	interiors.set_text(0, "Interiors")
	var worlds: Dictionary = {}
	for row: Dictionary in streamer.editor_cell_tree():
		var parent: TreeItem
		if row.interior:
			parent = interiors
		else:
			var world_id: String = row.world_id
			if not worlds.has(world_id):
				var world := tree.create_item(root)
				world.set_text(0, row.world_name if row.world_name != "" else world_id)
				worlds[world_id] = world
			parent = worlds[world_id]
		var item := tree.create_item(parent)
		var label: String = row.name
		if not row.interior:
			label += " (%d, %d)" % [row.x, row.y]
		item.set_text(0, "%s  [%d]" % [label, row.references])
		item.set_metadata(0, row.id)


func _open_selected_cell() -> void:
	var selected := tree.get_selected()
	if selected == null or not is_instance_valid(streamer):
		return
	var cell_id: Variant = selected.get_metadata(0)
	if not cell_id is String or cell_id.is_empty() or streamer.open_cell_id(cell_id) < 0:
		return
	var bounds: AABB = streamer.scene_bounds()
	if bounds.size.length_squared() > 0.0:
		var center := bounds.get_center()
		var distance: float = maxf(bounds.size.length(), 2.0)
		camera.position = center + Vector3(distance * 0.7, distance * 0.65, distance)
		camera.look_at(center)
