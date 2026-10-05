extends SceneTree

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _wait_for_ambience_count(bank: Node, expected: int) -> bool:
	for _frame in range(120):
		if int(bank.call("ambience_start_count")) >= expected:
			return true
		await process_frame
	return false

func _run() -> void:
	var extension_status := GDExtensionManager.load_extension("res://soe.gdextension")
	if extension_status != GDExtensionManager.LOAD_STATUS_OK and extension_status != GDExtensionManager.LOAD_STATUS_ALREADY_LOADED:
		printerr("FAIL: could not load the host extension, status ", extension_status)
		quit(1)
		return
	if not ClassDB.class_exists("SoundBank"):
		printerr("FAIL: SoundBank is not registered, the extension did not load")
		quit(1)
		return
	var data := OS.get_environment("AUD_09_TEST_DATA")
	if data.is_empty():
		printerr("FAIL: ambience fixture data was not supplied")
		quit(1)
		return

	var bank := ClassDB.instantiate("SoundBank") as Node3D
	get_root().add_child(bank)
	await process_frame
	_check(bank.call("open_data", data), "SoundBank could not open the ambience fixture: " + str(bank.call("last_error")))
	if failures != 0:
		quit(1)
		return

	var player := bank.get_node("AmbiencePlayer") as AudioStreamPlayer
	_check(bank.call("play_ambience", "Music/Cell.wav"), "eligible cell ambience did not start: " + str(bank.call("last_error")))
	_check(await _wait_for_ambience_count(bank, 1), "cell ambience decode did not finish")
	await process_frame
	_check(player.playing, "cell ambience AudioStreamPlayer is not playing after transition")
	_check(player.has_stream_playback(), "cell ambience has no Godot stream playback")

	bank.call("stop_ambience")
	await process_frame
	_check(not player.playing, "departed cell ambience AudioStreamPlayer kept playing")
	bank.queue_free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " AUD-09 ambience transition")
	quit(0 if failures == 0 else 1)
