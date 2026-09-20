extends SceneTree

## Real SNDR -> shared VFS -> decoded clip -> Godot mixer, without speakers.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("SoundBank"):
		printerr("FAIL: installed data and SoundBank extension required")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	var camera := Camera3D.new()
	get_root().add_child(camera)
	camera.make_current()
	var bank: Node3D = ClassDB.instantiate("SoundBank")
	get_root().add_child(bank)
	_check(bank.call("use_streamer_data", streamer), "sharing audio source failed")
	var capture := AudioEffectCapture.new()
	capture.buffer_length = 2.0
	AudioServer.add_bus_effect(0, capture)
	await process_frame
	# FSTPlayerWoodWalkLSD, an authored non-positional player output model.
	var slot: int = bank.call("play_descriptor", 0, 0x07F417, Vector3.ZERO, Vector3(0.5, 0.5, 0.5))
	_check(slot >= 0, "descriptor playback failed: " + str(bank.call("last_error")))
	await create_timer(0.6).timeout
	var samples := capture.get_buffer(capture.get_frames_available())
	var peak := 0.0
	for sample: Vector2 in samples:
		peak = maxf(peak, maxf(absf(sample.x), absf(sample.y)))
	_check(samples.size() > 1000, "mixer did not produce enough frames")
	_check(peak > 0.0001, "mixer output is silent")
	_check(bank.call("decoded_count") == 1, "first clip was not decoded exactly once")
	_check(bank.call("play_descriptor", 0, 0x07F417, Vector3.ZERO, Vector3(0.5, 0.5, 0.5)) >= 0,
		"cached descriptor playback failed")
	_check(bank.call("decoded_count") == 1, "second play decoded the cached clip again")
	print("SOUND MIXER: frames=", samples.size(), " peak=", peak,
		" decoded=", bank.call("decoded_count"))
	AudioServer.remove_bus_effect(0, AudioServer.get_bus_effect_count(0) - 1)
	bank.free()
	camera.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " sound descriptor")
	quit(0 if failures == 0 else 1)
