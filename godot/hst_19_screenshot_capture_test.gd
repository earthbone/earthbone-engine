extends SceneTree

## HST-19 headless check: call the Papyrus Debug.TakeScreenshot native through
## PlayerNode and confirm the host writes a PNG from the active viewport.
## Run with SKYRIM_DATA set and the GDExtension built.

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
		print("SKYRIM_DATA not set or extension not loaded, skipping")
		print("RESULT: PASS hst_19 screenshot capture (skipped)")
		quit(0)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	_check(streamer.call("stream_cell", CELL) > 0, "cell render failed")
	_check(streamer.call("stream_cell_collision", CELL) > 0, "cell collision failed")

	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data failed")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0, "player collision failed")
	_check(player.call("start_gameplay"), "game runtime failed")

	await process_frame
	await process_frame
	var path := OS.get_user_data_dir().path_join("hst_19_capture.png")
	_check(player.call("take_screenshot", path), "Papyrus screenshot request failed")
	_check(FileAccess.file_exists(path), "host did not create the requested path")
	if FileAccess.file_exists(path):
		var bytes := FileAccess.get_file_as_bytes(path)
		_check(bytes.size() > 24, "screenshot PNG was empty")
		_check(bytes.slice(0, 8) == PackedByteArray([137, 80, 78, 71, 13, 10, 26, 10]),
			"screenshot did not contain a PNG header")
		DirAccess.remove_absolute(path)

	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " hst_19 screenshot capture")
	quit(0 if failures == 0 else 1)
