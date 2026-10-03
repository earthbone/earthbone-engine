@tool
extends EditorPlugin

const ReferenceTarget = preload("res://addons/soe_reference_gizmo/reference_target.gd")
const ReferenceGizmo = preload("res://addons/soe_reference_gizmo/gizmo_plugin.gd")

var editor_document: Object
var gizmo_plugin: EditorNode3DGizmoPlugin
var dock: VBoxContainer
var plugin_paths: LineEdit
var active_plugin: LineEdit
var selected_form: LineEdit
var status: Label
var target: Node3D


func _enter_tree() -> void:
	editor_document = ClassDB.instantiate("SoeReferenceTransformEditor")
	gizmo_plugin = ReferenceGizmo.new()
	add_node_3d_gizmo_plugin(gizmo_plugin)
	dock = VBoxContainer.new()
	dock.name = "Skyrim Reference"
	plugin_paths = LineEdit.new()
	plugin_paths.placeholder_text = "Plugin paths in load order, separated by semicolons"
	dock.add_child(plugin_paths)
	active_plugin = LineEdit.new()
	active_plugin.placeholder_text = "Active plugin file name"
	dock.add_child(active_plugin)
	var open_button := Button.new()
	open_button.text = "Open load order"
	open_button.pressed.connect(_open_document)
	dock.add_child(open_button)
	selected_form = LineEdit.new()
	selected_form.placeholder_text = "Placed FormID, for example 2:000800"
	dock.add_child(selected_form)
	var attach_button := Button.new()
	attach_button.text = "Attach transform gizmo"
	attach_button.pressed.connect(_attach_selected_reference)
	dock.add_child(attach_button)
	var actions := HBoxContainer.new()
	var undo_button := Button.new()
	undo_button.text = "Undo record edit"
	undo_button.pressed.connect(_undo_record_edit)
	actions.add_child(undo_button)
	var save_button := Button.new()
	save_button.text = "Save plugin"
	save_button.pressed.connect(_save_plugin)
	actions.add_child(save_button)
	dock.add_child(actions)
	status = Label.new()
	dock.add_child(status)
	add_control_to_dock(DOCK_SLOT_LEFT_UL, dock)


func _exit_tree() -> void:
	remove_node_3d_gizmo_plugin(gizmo_plugin)
	remove_control_from_docks(dock)
	if is_instance_valid(dock):
		dock.queue_free()
	if is_instance_valid(target):
		target.queue_free()


func _open_document() -> void:
	var paths := PackedStringArray()
	for path in plugin_paths.text.split(";", false):
		paths.append(path.strip_edges())
	if editor_document.open(paths, active_plugin.text.strip_edges()):
		status.text = "Load order opened"
	else:
		status.text = editor_document.last_error()


func _attach_selected_reference() -> void:
	var root := EditorInterface.get_edited_scene_root()
	if root == null:
		status.text = "Open a scene before attaching a reference"
		return
	var candidate := ReferenceTarget.new()
	candidate.name = "SkyrimReference_%s" % selected_form.text.replace(":", "_")
	candidate.form_id = selected_form.text.strip_edges()
	candidate.editor_document = editor_document
	if not candidate.load_record_transform():
		candidate.free()
		status.text = "Could not read that placed reference"
		return
	if is_instance_valid(target):
		target.queue_free()
	target = candidate
	root.add_child(target)
	target.owner = root
	EditorInterface.edit_node(target)
	status.text = "Transform gizmo attached to %s" % target.form_id


func _undo_record_edit() -> void:
	if editor_document.undo():
		if is_instance_valid(target):
			target.load_record_transform()
			target.update_gizmos()
		status.text = "Record edit undone"
	else:
		status.text = "No record edit to undo"


func _save_plugin() -> void:
	status.text = "Plugin saved" if editor_document.save() else editor_document.last_error()
