extends SceneTree

## SCR-47 host check for loading and releasing vanilla Flames spell art.
const SPELL_FILE := 0
const FLAMES := 0x00012FCD

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: SCR-47 requires SKYRIM_DATA and the host extension")
		quit(1)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "installed game data did not load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "player did not share game data")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"player cell setup failed")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	if failures == 0:
		_check(player.call("scr_47_spell_art_call", SPELL_FILE, FLAMES, true),
			"Spell.Preload Papyrus call failed")
		player.call("advance", 1.0 / 60.0)
		_check(streamer.call("retained_spell_art_mesh_count_for_check") > 0,
			"Godot host did not retain spell art meshes")
		_check(player.call("scr_47_spell_art_call", SPELL_FILE, FLAMES, false),
			"Spell.Unload Papyrus call failed")
		player.call("advance", 1.0 / 60.0)
		_check(streamer.call("retained_spell_art_mesh_count_for_check") == 0,
			"Godot host did not release spell art meshes")

	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " SCR-47 spell art")
	quit(0 if failures == 0 else 1)
