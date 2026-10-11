extends SceneTree

## Verify Game's character-generation cache requests against streamer residency.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("CellStreamerNode") \
			or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: ui_17_chargen_cache_test requires SKYRIM_DATA and the extension")
		quit(1)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "streamer could not load installed data")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "player did not share streamer data")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"player could not load its game cell")
	_check(player.call("start_gameplay"), "game runtime did not start")

	_check(streamer.call("chargen_resident_count") == 0, "cache was resident before precache")
	_check(streamer.call("chargen_resident_bytes") == 0, "bytes were counted before precache")
	_check(player.call("debug_invoke_chargen_native", "PrecacheCharGen"),
		"PrecacheCharGen request did not complete")
	var files: int = streamer.call("chargen_resident_count")
	var models: int = streamer.call("chargen_model_count")
	var morphs: int = streamer.call("chargen_morph_count")
	var bytes: int = streamer.call("chargen_resident_bytes")
	_check(files > 0, "precache retained no asset files")
	_check(models > 0, "precache retained no head-part models")
	_check(morphs > 0, "precache retained no chargen morph files")
	_check(bytes > 0, "precache did not account resident bytes")
	_check(files == models + morphs, "file count differs from model and morph counts")
	_check(bytes >= files, "byte count is smaller than resident file count")
	_check(player.call("show_race_menu"), "race menu did not open after precache")
	_check(player.call("is_menu_open"), "race menu presenter left the menu closed")
	_check(streamer.call("chargen_resident_count") == files,
		"race menu opened after chargen assets left residency")
	player.call("close_menu")

	_check(player.call("debug_invoke_chargen_native", "PrecacheCharGenClear"),
		"PrecacheCharGenClear request did not complete")
	_check(streamer.call("chargen_resident_count") == 0, "clear left resident files")
	_check(streamer.call("chargen_model_count") == 0, "clear left resident models")
	_check(streamer.call("chargen_morph_count") == 0, "clear left resident morphs")
	_check(streamer.call("chargen_missing_count") == 0, "clear left missing path accounting")
	_check(streamer.call("chargen_resident_bytes") == 0, "clear left resident bytes")

	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ui_17 chargen cache")
	quit(0 if failures == 0 else 1)
