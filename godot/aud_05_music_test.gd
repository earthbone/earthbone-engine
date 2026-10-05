extends SceneTree

## Checks that interrupted music crossfades leave only the current two tracks playing.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _captured_peak(capture: AudioEffectCapture) -> float:
	var frames := capture.get_frames_available()
	if frames == 0:
		return 0.0
	var peak := 0.0
	for sample: Vector2 in capture.get_buffer(frames):
		peak = maxf(peak, maxf(absf(sample.x), absf(sample.y)))
	return peak

func _wait_music(bank: Node3D, path: String) -> void:
	var deadline := Time.get_ticks_msec() + 5_000
	while str(bank.call("current_music_path")) != path and Time.get_ticks_msec() < deadline:
		await process_frame
	_check(str(bank.call("current_music_path")) == path, "music did not become ready: " + path)

func _run() -> void:
	if not ClassDB.class_exists("SoundBank"):
		var extension := OS.get_environment("AUD_05_EXTENSION")
		var load_status := GDExtensionManager.load_extension(extension)
		if load_status != GDExtensionManager.LOAD_STATUS_OK:
			printerr("FAIL: GDExtension failed to load, status ", load_status)
			quit(1)
			return
	if not ClassDB.class_exists("SoundBank"):
		printerr("FAIL: SoundBank is not registered after loading the extension")
		quit(1)
		return
	var data := OS.get_environment("AUD_05_TEST_DATA")
	if data.is_empty():
		printerr("FAIL: music fixture data was not supplied")
		quit(1)
		return

	var bank := ClassDB.instantiate("SoundBank") as Node3D
	get_root().add_child(bank)
	await process_frame
	_check(bank.call("open_data", data), "SoundBank could not open music fixture data: " + str(bank.call("last_error")))
	if failures != 0:
		quit(1)
		return

	var capture := AudioEffectCapture.new()
	capture.buffer_length = 2.0
	AudioServer.add_bus_effect(0, capture)
	_check(bank.call("play_music_track", "music/aud05_first.wav", 2.0, 2.0, false), "first track did not start: " + str(bank.call("last_error")))
	await _wait_music(bank, "music/aud05_first.wav")
	var first_player := bank.get_child(16) as AudioStreamPlayer
	var second_player := bank.get_child(17) as AudioStreamPlayer
	_check(first_player.playing, "first track is not playing in Godot")
	var first_stream := first_player.stream

	_check(bank.call("play_music_track", "music/aud05_second.wav", 2.0, 2.0, false), "second track did not start: " + str(bank.call("last_error")))
	await _wait_music(bank, "music/aud05_second.wav")
	_check(first_player.playing and second_player.playing, "first crossfade did not play both tracks")
	await create_timer(0.12).timeout
	_check(first_player.volume_db < 0.0, "outgoing track did not apply the authored fade")
	_check(second_player.volume_db < 0.0, "incoming track did not apply the MUSC fade")

	_check(bank.call("play_music_track", "music/aud05_third.wav", 2.0, 2.0, false), "interrupted transition did not start: " + str(bank.call("last_error")))
	await _wait_music(bank, "music/aud05_third.wav")
	_check(first_player.playing and second_player.playing, "replacement transition did not retain its two active tracks")
	_check(first_player.stream != first_stream, "stale first stream remains assigned after interruption")
	await create_timer(0.15).timeout
	var peak := _captured_peak(capture)
	_check(peak > 0.0001, "music players produced no Godot mixer output")

	first_player.stop()
	second_player.stop()
	AudioServer.remove_bus_effect(0, AudioServer.get_bus_effect_count(0) - 1)
	bank.queue_free()
	print("AUD-05 mixer peak: ", peak)
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " music playback and interrupted transition")
	quit(0 if failures == 0 else 1)
