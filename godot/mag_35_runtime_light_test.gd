extends SceneTree

## MAG-35 live RenderingServer check using Skyrim's Candlelight spell.
var failures := 0
const CELL := "WhiterunBanneredMare"
const CANDLELIGHT := 0x00043324

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: MAG-35 requires SKYRIM_DATA and the host extension")
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
	if failures == 0:
		_check(player.call("cast_spell_on_self", 0, CANDLELIGHT),
			"vanilla Candlelight did not cast on the player")
		player.call("advance", 1.0 / 60.0)
		_check(streamer.call("runtime_light_count") == 1,
			"RenderingServer did not create the active Candlelight")
		player.call("advance", 61.0)
		player.call("advance", 1.0 / 60.0)
		_check(streamer.call("runtime_light_count") == 0,
			"RenderingServer light survived past the authored Candlelight duration")

	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " MAG-35 runtime light")
	quit(0 if failures == 0 else 1)
