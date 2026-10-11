extends SceneTree

const PAGE_SIZE := 200


static func edt_11_live_selection(forms_plugin: EditorPlugin) -> String:
	var table: Variant = forms_plugin.get("form_table")
	var results: Tree = forms_plugin.get("results")
	var page := 0
	var selected_id := ""
	var expected: Array[Dictionary] = []
	while true:
		var ids: PackedStringArray = table.search("", "", page * PAGE_SIZE, PAGE_SIZE)
		if ids.is_empty():
			break
		for form_id: String in ids:
			var chain: Array[Dictionary] = table.override_chain(form_id)
			if chain.size() > 1:
				selected_id = form_id
				expected = chain
				break
		if not selected_id.is_empty():
			forms_plugin.set("page_index", page)
			forms_plugin.call("_show_page")
			break
		page += 1
	if selected_id.is_empty():
		return "the installed load order contains no overridden record"
	var item := results.get_root().get_first_child()
	while item != null and item.get_metadata(0) != selected_id:
		item = item.get_next()
	if item == null:
		return "the selected conflict was absent from the forms dock results"
	results.set_selected(item, 0)
	forms_plugin.call("_select_form")
	await Engine.get_main_loop().process_frame
	var dock: Tree = forms_plugin.get("override_chain")
	var row := dock.get_root().get_first_child()
	if row == null:
		return "selecting the form did not populate the override-chain dock"
	for version: Dictionary in expected:
		if row == null:
			return "the override-chain dock omitted a load-order version"
		if row.get_text(0) != version.get("plugin_name", ""):
			return "the dock plugin order differs from the opened load order"
		if row.get_text(1) != version.get("signature", ""):
			return "the dock record signature differs from the selected record"
		if row.get_text(2) != version.get("editor_id", ""):
			return "the dock editor id differs from the selected record version"
		var expected_winner := "Yes" if version.get("is_winner", false) else "No"
		if row.get_text(3) != expected_winner:
			return "the dock winner marker differs from the resolved load-order winner"
		row = row.get_next()
	if row != null:
		return "the override-chain dock contains versions outside the selected load order"
	if expected.back().get("is_winner", false) != true:
		return "the last record version was not reported as the winner"
	return ""


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var view_script = load("res://addons/soe_forms/override_chain_view.gd")
	var view: Tree = view_script.new()
	root.add_child(view)
	var versions: Array[Dictionary] = [
		{
			"plugin_name": "Base.esm",
			"signature": "STAT",
			"editor_id": "BaseStatue",
			"is_winner": false,
		},
		{
			"plugin_name": "Final.esp",
			"signature": "STAT",
			"editor_id": "FinalStatue",
			"is_winner": true,
		},
	]
	view.show_chain(versions)
	var row := view.get_root().get_first_child()
	if view.columns != 4 or row == null:
		quit(1)
		return
	if row.get_text(0) != "Base.esm" or row.get_text(1) != "STAT":
		quit(1)
		return
	if row.get_text(2) != "BaseStatue" or row.get_text(3) != "No":
		quit(1)
		return
	row = row.get_next()
	if row == null or row.get_text(0) != "Final.esp":
		quit(1)
		return
	if row.get_text(2) != "FinalStatue" or row.get_text(3) != "Yes":
		quit(1)
		return
	var no_versions: Array[Dictionary] = []
	view.show_chain(no_versions)
	if view.get_root().get_first_child() != null:
		quit(1)
		return
	print("edt_11_override_chain_dock: passed")
	quit(0)
