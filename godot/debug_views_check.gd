extends "res://walkthrough.gd"

## Installed-data diagnostic check over the real walkthrough host and Console.
var diagnostics: RichTextLabel
var messages: Array[String] = []

func _start() -> void:
	await super._start()
	if not loaded:
		return
	var canvas := CanvasLayer.new()
	diagnostics = RichTextLabel.new()
	diagnostics.size = Vector2(900, 600)
	diagnostics.bbcode_enabled = false
	canvas.add_child(diagnostics)
	get_root().add_child(canvas)
	player.connect("debug_view_message", _diagnostic_message)
	var deadline := Time.get_ticks_msec() + 60000
	while int(player.call("npc_visual_counts").drawn) == 0 and Time.get_ticks_msec() < deadline:
		await process_frame
	if not _require(int(player.call("npc_visual_counts").drawn) > 0, "resident animation node"):
		return
	if not _require(player.call("debug_geometry_counts").is_empty(), "off by default"):
		return
	_command("tnm")
	await process_frame
	var counts: Dictionary = player.call("debug_geometry_counts")
	var geometry: MeshInstance3D = player.get_node("SoeDebugGeometry")
	var vertices: PackedVector3Array = geometry.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX]
	if not _require(int(counts.triangles) > 100 and int(counts.triangles) == int(counts.loaded_navmesh_triangles) and vertices.size() == int(counts.triangles) * 3, "tnm uploaded loaded NAVM triangles"):
		return
	_command("tpl")
	await process_frame
	counts = player.call("debug_geometry_counts")
	print("DEBUG_VIEWS path source ", JSON.stringify(counts))
	if not _require(int(counts.path_segments) == int(counts.actor_path_points) and int(counts.waypoints) == int(counts.actor_path_points), "tpl live path counts"):
		return
	print("DEBUG_VIEWS geometry ", JSON.stringify(counts))
	if not _require(_command("pai").contains("procedure="), "PrintAiList overlay"):
		return
	var actor: Dictionary
	for row in player.call("npc_visuals"):
		if int(row.shapes) > 0:
			actor = row
			break
	var runtime_id: int = (int(actor.file) << 24) | int(actor.object)
	_command("prid %x" % runtime_id)
	_command("sa")
	for _frame in range(10):
		await process_frame
	var animation := _command("sa")
	if not _require(animation.contains("authored_duration=") and animation.contains("measured_playback_rate=") and animation.contains("clock_step=") and animation.contains("updates_in_host_frame="), "ShowAnim live clock report"):
		return
	print("DEBUG_VIEWS animation ", animation)
	var actor_node: Node = instance_from_id(int(actor.node_id))
	actor_node.set("playing", false)
	await process_frame
	actor_node.call("advance", 0.1)
	actor_node.call("advance", 0.1)
	var doubled_clock := _command("sa")
	if not _require(doubled_clock.contains("updates_in_host_frame=2") and doubled_clock.contains("frame_clock_sum=0.200000"), "ShowAnim detects two clock advances in one frame"):
		return
	actor_node.set("playing", true)
	var tree := _command("ssg")
	if not _require(tree.contains("local=") and tree.contains("accumulated=") and tree.contains("host_capsule radius="), "ShowScenegraph transforms and live capsule"):
		return
	var dialogue := _command("pdialog")
	if not _require(dialogue.contains("CTDA[") and dialogue.contains("result="), "PrintNPCDialog candidate conditions"):
		return
	_command("tsd")
	_command("swdp")
	_command("sp")
	_command("sp")
	_command("tdt")
	_command("tdt")
	if not _require(_command("tmc").contains("map camera on"), "ToggleMapCam on"):
		return
	if not _require(_command("tmc").contains("map camera off"), "ToggleMapCam restore"):
		return
	_command("tlm 0")
	_command("tmc")
	var hdr := _command("thd")
	if hdr.contains("HDR luminance debug on"):
		if not _require(_command("thd").contains("HDR luminance debug off"), "ToggleHDRDebug restore"):
			return
	elif not _require(hdr.contains("needs Godot Forward+ or Mobile"), "ToggleHDRDebug explicit renderer limitation"):
		return
	_command("TPLog")
	_command("player.callfunc DebugViewsMissingNative")
	for _frame in range(3):
		await process_frame
	var log_path := ProjectSettings.globalize_path("user://debug-views/Logs/Script/Papyrus.0.log")
	if not _require(FileAccess.file_exists(log_path) and FileAccess.get_file_as_string(log_path).contains("DebugViewsMissingNative"), "TogglePapyrusLog file"):
		return
	if not _require("\n".join(messages).contains("[Papyrus"), "TogglePapyrusLog overlay"):
		return
	_command("TPLog")
	_command("tnm")
	_command("tpl")
	await process_frame
	await process_frame
	if not _require(player.call("debug_geometry_counts").is_empty() and not player.has_node("SoeDebugGeometry"), "debug nodes freed when off"):
		return
	if not _require(diagnostics.get_parsed_text().contains("procedure=") and diagnostics.get_parsed_text().contains("CTDA["), "console overlay received reports"):
		return
	print("DEBUG_VIEWS passed")
	quit(0)

func _command(command: String) -> String:
	return str(player.call("debug_view_command", command))

func _diagnostic_message(message: String) -> void:
	messages.append(message)
	diagnostics.add_text(message + "\n")

func _require(passed: bool, check: String) -> bool:
	print("DEBUG_VIEWS ", check, " ", passed)
	if not passed:
		quit(1)
	return passed
