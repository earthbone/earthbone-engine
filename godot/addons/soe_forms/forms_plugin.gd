@tool
extends EditorPlugin

var dock: VBoxContainer
var load_order_paths: LineEdit
var active_plugin: LineEdit
var signature_filter: LineEdit
var text_filter: LineEdit
var status: Label
var results: Tree
var form_table: SoeFormTable
var form_inspector: EditorInspectorPlugin
var page_index := 0
const PAGE_SIZE := 200


func _enter_tree() -> void:
	form_inspector = preload("res://addons/soe_forms/form_inspector.gd").new()
	add_inspector_plugin(form_inspector)
	if OS.get_environment("SOE_EDT13_INSPECTOR_TEST") == "1":
		call_deferred("_run_edt_13_inspector_test")
	form_table = SoeFormTable.new()
	dock = VBoxContainer.new()
	dock.name = "Skyrim Forms"
	load_order_paths = LineEdit.new()
	load_order_paths.placeholder_text = "Plugin paths in load order, separated by semicolons"
	dock.add_child(load_order_paths)
	active_plugin = LineEdit.new()
	active_plugin.placeholder_text = "Active plugin file name"
	dock.add_child(active_plugin)
	var open_button := Button.new()
	open_button.text = "Open load order"
	open_button.pressed.connect(_open_load_order)
	dock.add_child(open_button)
	signature_filter = LineEdit.new()
	signature_filter.placeholder_text = "Record type, for example STAT"
	signature_filter.max_length = 4
	signature_filter.text_changed.connect(_refresh)
	dock.add_child(signature_filter)
	text_filter = LineEdit.new()
	text_filter.placeholder_text = "Search Editor ID"
	text_filter.text_changed.connect(_refresh)
	dock.add_child(text_filter)
	var page_controls := HBoxContainer.new()
	var previous_page := Button.new()
	previous_page.text = "Previous"
	previous_page.pressed.connect(_previous_page)
	page_controls.add_child(previous_page)
	var next_page := Button.new()
	next_page.text = "Next"
	next_page.pressed.connect(_next_page)
	page_controls.add_child(next_page)
	dock.add_child(page_controls)
	results = Tree.new()
	results.hide_root = true
	results.size_flags_vertical = Control.SIZE_EXPAND_FILL
	results.item_selected.connect(_select_form)
	dock.add_child(results)
	status = Label.new()
	dock.add_child(status)
	add_control_to_dock(EditorPlugin.DOCK_SLOT_LEFT_UL, dock)


func _exit_tree() -> void:
	remove_inspector_plugin(form_inspector)
	remove_control_from_docks(dock)
	dock.queue_free()
	form_inspector = null
	form_table = null


func _open_load_order() -> void:
	var paths := PackedStringArray()
	for path in load_order_paths.text.split(";", false):
		paths.append(path.strip_edges())
	if form_table.open(paths, active_plugin.text.strip_edges()):
		status.text = "Load order opened"
		_refresh("")
	else:
		status.text = form_table.last_error()


func _refresh(_changed: String) -> void:
	page_index = 0
	_show_page()


func _show_page() -> void:
	results.clear()
	var root := results.create_item()
	for form_id in form_table.search(
		signature_filter.text.strip_edges(), text_filter.text, page_index * PAGE_SIZE, PAGE_SIZE
	):
		var item := results.create_item(root)
		item.set_text(0, form_id)
		item.set_metadata(0, form_id)
	status.text = "Page %d, %d results" % [page_index + 1, results.get_root().get_child_count()]


func _previous_page() -> void:
	page_index = maxi(page_index - 1, 0)
	_show_page()


func _next_page() -> void:
	page_index += 1
	_show_page()


func _select_form() -> void:
	var item := results.get_selected()
	if item == null:
		return
	var form_resource := form_table.selected_form(item.get_metadata(0))
	if form_resource != null:
		EditorInterface.edit_resource(form_resource)
		status.text = "Inspector resources created: %d" % form_table.resources_created()


func _run_edt_13_inspector_test() -> void:
	var test_script = load("res://edt_13_inspector_test.gd")
	if await test_script.edt_13_inspector_displays_winning_source(form_inspector):
		print("RESULT: PASS: Godot inspector winning source test")
	else:
		push_error("RESULT: FAIL: Godot inspector winning source test")
		get_tree().quit(1)
