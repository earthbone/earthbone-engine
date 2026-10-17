extends "res://walkthrough.gd"

## One walkthrough startup per cell. The runner retains the process exit status.
var sweep_cell: Dictionary
var output_path: String
var outcomes: Array = []
var initial: Dictionary = {}

func _initialize() -> void:
	var input_path := OS.get_environment("SOE_ACTIVATION_CELL")
	sweep_cell = JSON.parse_string(FileAccess.get_file_as_string(input_path))
	startup_cell = sweep_cell.cell.get("editor_id", "")
	if startup_cell == null or startup_cell.is_empty():
		startup_cell = sweep_cell.cell.id
	output_path = OS.get_environment("SOE_ACTIVATION_OUTPUT")
	super._initialize()

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _process(_delta: float) -> bool:
	return false

func _save(complete: bool) -> void:
	var file := FileAccess.open(output_path, FileAccess.WRITE)
	file.store_string(JSON.stringify({"cell": sweep_cell.cell, "complete": complete, "initial": initial, "references": outcomes}, "  "))
	file.close()

func _deltas(before: Dictionary, after: Dictionary) -> Dictionary:
	var result := {}
	for field: String in ["papyrus_errors", "native_errors", "animation_requests"]:
		var prior = before.get(field, [])
		var current = after.get(field, [])
		if outcomes.is_empty():
			initial[field] = prior
		result[field] = Array(current).slice(prior.size())
		before[field + "_count"] = prior.size()
		after[field + "_count"] = current.size()
		before.erase(field)
		after.erase(field)
	return result

func _start() -> void:
	await super._start()
	if not loaded:
		quit(1)
		return
	for row: Dictionary in sweep_cell.references:
		var file := int(row.reference.file)
		var object := int(row.reference.object)
		if player.call("is_menu_open"):
			player.call("close_menu")
		var watched: bool = player.call("debug_watch_activation", file, object)
		var before: Dictionary = player.call("activation_reference_status", file, object)
		var transforms: Array = streamer.call("reference_animation_transforms", file, object)
		var drawn: int = streamer.call("rendered_reference_count", file, object)
		var accepted: bool = player.call("activate_reference", file, object)
		var moved := false
		for step in 12:
			player.call("advance", 0.25)
			streamer.call("step_object_animations", 0.25)
			var current: Array = streamer.call("reference_animation_transforms", file, object)
			moved = moved or (not transforms.is_empty() and current != transforms)
			await process_frame
		var after: Dictionary = player.call("activation_reference_status", file, object)
		var menu: bool = player.call("is_menu_open")
		var causes: Array[String] = []
		if not watched or after.get("activations", 0) != before.get("activations", 0) + 1:
			causes.append("host_activation_event")
		if after.get("papyrus_errors", []).size() > before.get("papyrus_errors", []).size() or after.get("native_errors", []).size() > before.get("native_errors", []).size():
			causes.append("host_papyrus_error")
		if (not row.sequences.is_empty() or not row.animation_requests.is_empty()) and not moved:
			causes.append("host_animation_no_motion")
		if row.base_type in ["CONT", "NPC_"] and not before.get("locked", false) and not before.get("blocked", false) and not menu:
			causes.append("host_menu_missing")
		var result := {"reference": row.reference, "accepted": accepted,
			"before": before, "after": after, "menu": menu,
			"drawn_surfaces": drawn, "animation_surfaces": transforms.size(), "motion": moved,
			"causes": causes, "status": "observed" if causes.is_empty() else "fail"}
		result.merge(_deltas(before, after))
		outcomes.append(result)
		_save(false)
	_save(true)
	print("ACTIVATION_SWEEP complete refs=", outcomes.size(), " cell=", startup_cell)
	quit(0)
