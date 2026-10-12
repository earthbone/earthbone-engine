extends SceneTree

## HST-19 host check: call the Papyrus Debug.TakeScreenshot native through
## PlayerNode and confirm the host writes a PNG from the active viewport.
## Run headless with SKYRIM_DATA set after building the release GDExtension.

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
	if data.is_empty():
		printerr("SKYRIM_DATA is required for the installed host capture")
		quit(1)
		return
	if not ClassDB.class_exists("PlayerNode"):
		printerr("PlayerNode extension class is not available")
		quit(1)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	_check(streamer.call("stream_cell", CELL) > 0, "cell render failed")
	_check(streamer.call("stream_cell_collision", CELL) > 0, "cell collision failed")

	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data failed")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0, "player collision failed")
	_check(player.call("start_gameplay"), "game runtime failed")

	await process_frame
	await process_frame
	var path := "/tmp/hst_19_capture_%d.png" % OS.get_process_id()
	_check(player.call("take_screenshot", path), "Papyrus screenshot request failed")
	for _frame in range(120):
		if FileAccess.file_exists(path):
			break
		await process_frame
	_check(FileAccess.file_exists(path), "host did not create the requested path")
	if FileAccess.file_exists(path):
		var bytes := FileAccess.get_file_as_bytes(path)
		_check(bytes.size() > 24, "screenshot PNG was empty")
		_check(bytes.slice(0, 8) == PackedByteArray([137, 80, 78, 71, 13, 10, 26, 10]),
			"screenshot did not contain a PNG header")
		var image := Image.load_from_file(path)
		_check(not image.is_empty(), "screenshot PNG could not be decoded")
		if not image.is_empty():
			_check(image.get_width() == 640 and image.get_height() == 360,
				"screenshot dimensions did not match the host viewport")
		DirAccess.remove_absolute(path)

	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " hst_19 screenshot capture")
	quit(0 if failures == 0 else 1)
