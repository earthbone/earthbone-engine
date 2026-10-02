extends SceneTree


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
