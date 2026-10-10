extends SceneTree

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		printerr("SKYRIM_DATA is not set")
		quit(1)
		return
	var streamer: Node = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "installed data did not load")
	var result: Dictionary = streamer.call("cmb_37_damage_and_sync_model")
	_check(not result.has("error"), "damage and host sync failed: %s" % result)
	if not result.has("error"):
		_check(int(result.get("stage", -1)) >= 0, "damage did not advance a DEST stage")
		_check(not String(result.get("model", "")).is_empty(), "stage has no selected model")
		_check(int(result.get("instances", 0)) > 0, "replacement created no RenderingServer instances")
		print("CMB_37_RENDER base=%s reference=%s stage=%s model=%s instances=%s" % [
			result.get("base"), result.get("reference"), result.get("stage"),
			result.get("model"), result.get("instances")
		])
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " CMB-37 destruction render")
	quit(0 if failures == 0 else 1)
