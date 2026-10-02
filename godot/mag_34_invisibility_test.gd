extends SceneTree

## MAG-34 live visibility check using the installed Invisibility spell.
const PLAYER_ACTOR := 0x14
const INVISIBILITY := 0x00027EB6
const BODY := "meshes/actors/character/character assets/malebody_1.nif"
const CELL := "WhiterunBanneredMare"

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
		printerr("FAIL: MAG-34 requires SKYRIM_DATA and the host extension")
		quit(1)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "installed data load failed")
	_check(streamer.call("stream_cell", CELL) > 0, "cell render load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source failed")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0,
		"player cell load failed")
	_check(player.call("start_gameplay"), "gameplay startup failed")

	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	get_root().add_child(visual)
	_check(visual.call("load_character", data, BODY), "player actor mesh failed to load")
	_check(player.call("bind_actor_appearance", visual, 0, PLAYER_ACTOR),
		"player appearance binding failed")
	var stats: Dictionary = visual.call("stats")
	_check(int(stats.get("shapes", 0)) > 0, "player appearance has no rendered shapes")
	_check(not bool(stats.get("invisible", true)), "player starts invisible")

	if failures == 0:
		_check(player.call("cast_spell_on_self", 0, INVISIBILITY),
			"vanilla Invisibility did not cast on the player")
		player.call("advance", 1.0 / 60.0)
		visual.call("advance", 1.0 / 60.0)
		await process_frame
		stats = visual.call("stats")
		_check(bool(stats.get("invisible", false)),
			"active Invisibility did not hide the RenderingServer actor instances")
		player.call("advance", 31.0)
		visual.call("advance", 1.0 / 60.0)
		await process_frame
		stats = visual.call("stats")
		_check(not bool(stats.get("invisible", true)),
			"expired Invisibility did not restore actor visibility")

	visual.free()
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " MAG-34 invisibility")
	quit(0 if failures == 0 else 1)
