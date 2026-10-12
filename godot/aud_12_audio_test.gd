extends SceneTree

var failures := 0


func _initialize() -> void:
	call_deferred("_run")


func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("AUD-12 FAIL: ", message)


func wait_path(bank: Node3D, lane: String, path: String) -> void:
	var deadline := Time.get_ticks_msec() + 5_000
	while str(bank.call("current_" + lane + "_path")) != path and Time.get_ticks_msec() < deadline:
		await process_frame
	check(str(bank.call("current_" + lane + "_path")) == path, "decode did not become ready: " + path)


func _run() -> void:
	var bank := ClassDB.instantiate("SoundBank") as Node3D
	root.add_child(bank)
	await process_frame
	check(bank.call("open_data", OS.get_environment("AUD_12_TEST_DATA")), "fixture data opens")
	var effect := int(bank.call("play_effect", "music/first.wav", Vector3.ZERO))
	var voice := int(bank.call("play_voice", "music/first.wav", Vector3.ZERO))
	check(effect >= 0 and voice >= 0, "effects and voices keep synchronous playback")
	check((bank.get_child(effect) as AudioStreamPlayer3D).stream is AudioStreamWAV, "effect keeps WAV path")
	check((bank.get_child(voice) as AudioStreamPlayer3D).stream is AudioStreamWAV, "voice keeps WAV path")
	check(int(bank.call("decoded_count")) == 1, "effects and voices share their existing cache")
	(bank.get_child(effect) as AudioStreamPlayer3D).stop()
	bank.call("stop_voice", voice)

	var capture := AudioEffectCapture.new()
	capture.buffer_length = 0.25
	AudioServer.add_bus_effect(0, capture)
	check(bank.call("play_music_track", "music/first.wav", 0.0, 0.0, true), "music request accepted")
	check(bank.call("play_ambience", "music/first.wav"), "ambience request accepted")
	check(int(bank.call("music_start_count")) == 0, "music waits for a completion")
	await wait_path(bank, "music", "music/first.wav")
	await wait_path(bank, "ambience", "music/first.wav")
	var first := bank.get_child(16) as AudioStreamPlayer
	var second := bank.get_child(17) as AudioStreamPlayer
	var ambience := bank.get_node("AmbiencePlayer") as AudioStreamPlayer
	check(first.stream is AudioStreamGenerator and ambience.stream is AudioStreamGenerator, "background tracks use chunked generators")
	await create_timer(0.12).timeout
	var peak := 0.0
	var stereo_difference := 0.0
	for sample: Vector2 in capture.get_buffer(capture.get_frames_available()):
		peak = maxf(peak, maxf(absf(sample.x), absf(sample.y)))
		stereo_difference = maxf(stereo_difference, absf(sample.x - sample.y))
	check(peak > 0.01 and stereo_difference > 0.01, "generator reaches the mixer and preserves music stereo")
	var playback := first.get_stream_playback() as AudioStreamGeneratorPlayback
	check(playback.get_skips() == 0, "chunked playback has no buffer underrun")

	check(bank.call("play_music_track", "music/second.wav", 0.0, 0.0, true), "second request accepted")
	check(first.playing and str(bank.call("current_music_path")) == "music/first.wav", "current music keeps playing while replacement is pending")
	check(bank.call("play_music_track", "music/third.wav", 0.0, 0.0, true), "latest request supersedes second")
	await wait_path(bank, "music", "music/third.wav")
	check(int(bank.call("music_start_count")) == 2, "superseded music never starts")
	check(second.playing and not first.playing, "ready abrupt transition releases previous music")
	var decodes := int(bank.call("decoded_count"))
	check(bank.call("play_music_track", "MUSIC\\FIRST.WAV", 0.0, 0.0, true), "cached music request accepted")
	await wait_path(bank, "music", "music/first.wav")
	check(int(bank.call("decoded_count")) == decodes, "cached normalized track skips decoding")
	check(int(bank.call("background_cache_bytes")) <= 64 * 1024 * 1024, "PCM cache stays capped")

	check(bank.call("play_ambience", "music/second.wav"), "ambience replacement accepted")
	check(ambience.playing and str(bank.call("current_ambience_path")) == "music/first.wav", "current ambience remains until replacement is ready")
	bank.call("stop_ambience")
	await create_timer(0.1).timeout
	check(not ambience.playing and int(bank.call("ambience_start_count")) == 1, "stop cancels pending ambience")
	check(not bank.call("play_music_track", "music/absent.wav", 0.0, 0.0, true), "missing indexed paths fail immediately")
	check(bank.call("play_music_track", "music/broken.wav", 0.0, 0.0, true), "decode failure is asynchronous")
	var deadline := Time.get_ticks_msec() + 5_000
	while str(bank.call("last_error")).is_empty() and Time.get_ticks_msec() < deadline:
		await process_frame
	check(not str(bank.call("last_error")).is_empty() and first.playing, "decode failure retains current music and reports its error")
	bank.call("play_music_track", "music/second.wav", 0.0, 0.0, true)
	bank.call("stop_music")
	await create_timer(0.1).timeout
	check(not first.playing and not second.playing, "stop cancels pending music and releases both slots")

	bank.call("play_music_track", "music/short.wav", 0.0, 0.0, true)
	await wait_path(bank, "music", "music/short.wav")
	await create_timer(0.35).timeout
	check(not first.playing and not second.playing, "generator stops after its finite PCM drains")
	AudioServer.remove_bus_effect(0, AudioServer.get_bus_effect_count(0) - 1)
	bank.queue_free()
	print("aud_12_generator_playback_cache_cancellation_and_errors: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
