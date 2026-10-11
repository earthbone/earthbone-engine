extends SceneTree

## A vanilla ESS startup passes its filename character ID into the live Game.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _first_save(directory: String) -> String:
	var files := DirAccess.get_files_at(directory)
	files.sort()
	for filename: String in files:
		if filename.ends_with(".ess"):
			return directory.path_join(filename)
	return ""

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var saves := OS.get_environment("SOE_TEST_SAVES")
	if data.is_empty() or saves.is_empty() or not ClassDB.class_exists("PlayerNode"):
		print("SKIP mod_01_vanilla_save_seed_test: installed data, SOE_TEST_SAVES or extension unavailable")
		quit(0)
		return
	var save_path := _first_save(saves.path_join("Clean Saves"))
	_check(not save_path.is_empty(), "SOE_TEST_SAVES has no ESS file in Clean Saves")
	if save_path.is_empty():
		quit(1)
		return
	var filename := save_path.get_file().get_basename()
	var expected_id := "0000000000000000"
	var fields := filename.split("_")
	if fields.size() == 9 and fields[1].is_valid_hex_number():
		expected_id = fields[1].to_upper().pad_zeros(16)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "installed Skyrim data did not load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "player did not share installed data")
	var result: Dictionary = player.call("start_gameplay_from_vanilla_save", save_path)
	_check(result.get("ok", false), "vanilla ESS startup failed: " + str(result))
	_check(result.get("state_loaded", false), "ESS state was not restored")
	_check(result.get("save_character_id", "") == expected_id,
		"ESS filename character ID did not reach the gameplay seed: " + str(result))
	await process_frame
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " mod_01 vanilla save seed")
	quit(0 if failures == 0 else 1)
