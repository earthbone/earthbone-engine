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
var page_index := 0
const PAGE_SIZE := 200


func _enter_tree() -> void:
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
	if OS.get_environment("SOE_EDT_02_RUNTIME_TEST") == "1":
		_run_runtime_check.call_deferred()


func _exit_tree() -> void:
	remove_control_from_docks(dock)
	dock.queue_free()
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


func _run_runtime_check() -> void:
	var base_path := OS.get_environment("SOE_EDT_02_BASE")
	var patch_path := OS.get_environment("SOE_EDT_02_PATCH")
	var active_name := OS.get_environment("SOE_EDT_02_ACTIVE")
	if base_path.is_empty() or patch_path.is_empty() or active_name.is_empty():
		_finish_runtime_check(false, "fixture paths were not supplied")
		return
	load_order_paths.text = base_path + ";" + patch_path
	active_plugin.text = active_name
	if not form_table.open(PackedStringArray([base_path, patch_path]), active_name):
		_finish_runtime_check(false, "fixture load order did not open: " + form_table.last_error())
		return
	signature_filter.text = "STAT"
	text_filter.text = "oAk"
	_show_page()
	var root := results.get_root()
	if root == null or root.get_child_count() != 2:
		_finish_runtime_check(false, "dock did not show the two filtered winning STAT forms")
		return
	if root.get_first_child().get_text(0) != "0:000800" or root.get_first_child().get_next().get_text(0) != "0:000801":
		_finish_runtime_check(false, "dock results did not contain the expected sorted winning FormIDs")
		return
	if form_table.resources_created() != 0:
		_finish_runtime_check(false, "search created an inspector Resource")
		return
	var selected := root.get_first_child()
	results.set_selected(selected, 0)
	if form_table.resources_created() == 0:
		_select_form()
	if form_table.resources_created() != 1:
		_finish_runtime_check(false, "selecting one result did not create exactly one Resource")
		return
	print("EDT-02 PASS: dock filtered loaded forms and created one Resource on selection")
	get_tree().quit(0)


func _finish_runtime_check(passed: bool, detail: String) -> void:
	if passed:
		get_tree().quit(0)
	else:
		printerr("EDT-02 FAIL: " + detail)
		get_tree().quit(1)
