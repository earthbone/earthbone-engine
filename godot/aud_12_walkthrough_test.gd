extends "res://walkthrough.gd"

const AUDIO_LIMIT_MS := 2.0
var aud_12_failures := 0
var aud_12_stage := 0
var aud_12_frames := 0
var aud_12_worst_ms := 0.0
var aud_12_deadline := 0
var aud_12_old_path := ""
var aud_12_first_path := ""
var aud_12_second_path := ""
var aud_12_decodes := 0
var aud_12_starts := 0
var aud_12_capture: AudioEffectCapture
var aud_12_peak := 0.0


func _process(delta: float) -> bool:
	if not loaded:
		return super._process(delta)
	aud_12_frames += 1
	var report: Dictionary = player.call("system_frame_breakdown")
	aud_12_worst_ms = maxf(aud_12_worst_ms, float(report.audio.worst_ms))
	var bank := player.get_node("SoundBank")
	if aud_12_deadline == 0:
		aud_12_deadline = Time.get_ticks_msec() + 120_000
		aud_12_first_path = OS.get_environment("AUD_12_MUSIC_A")
		aud_12_second_path = OS.get_environment("AUD_12_MUSIC_B")
		aud_12_capture = AudioEffectCapture.new()
		aud_12_capture.buffer_length = 0.25
		AudioServer.add_bus_effect(0, aud_12_capture)
	for sample: Vector2 in aud_12_capture.get_buffer(aud_12_capture.get_frames_available()):
		aud_12_peak = maxf(aud_12_peak, maxf(absf(sample.x), absf(sample.y)))
	if aud_12_worst_ms > AUDIO_LIMIT_MS:
		_fail("audio exceeded 2 ms: " + str(aud_12_worst_ms))
		return false
	if aud_12_stage != 4 and Time.get_ticks_msec() > aud_12_deadline:
		_fail("timed out waiting for music or ambience: " + str(bank.call("last_error")))
		return false
	var music_path := str(bank.call("current_music_path"))
	if aud_12_stage == 0 and int(bank.call("music_start_count")) > 0 and int(bank.call("ambience_start_count")) > 0:
		if music_path == aud_12_first_path:
			var swap := aud_12_first_path
			aud_12_first_path = aud_12_second_path
			aud_12_second_path = swap
		aud_12_starts = int(bank.call("music_start_count"))
		aud_12_old_path = music_path
		_request(bank, aud_12_first_path)
		aud_12_stage = 1
	elif aud_12_stage == 1 and music_path == aud_12_first_path:
		aud_12_old_path = music_path
		_request(bank, aud_12_second_path)
		aud_12_stage = 2
	elif aud_12_stage == 2 and music_path == aud_12_second_path:
		aud_12_decodes = int(bank.call("decoded_count"))
		aud_12_old_path = music_path
		_request(bank, aud_12_first_path)
		aud_12_stage = 3
	elif aud_12_stage == 3 and music_path == aud_12_first_path:
		if int(bank.call("decoded_count")) != aud_12_decodes:
			_fail("cached music decoded again")
			return false
		aud_12_stage = 4
		aud_12_deadline = Time.get_ticks_msec() + 2_000
	elif aud_12_stage == 4 and Time.get_ticks_msec() >= aud_12_deadline:
		if aud_12_peak < 0.001 or int(bank.call("music_start_count")) != aud_12_starts + 3 or int(bank.call("background_cache_bytes")) > 64 * 1024 * 1024:
			_fail("mixer output, track changes or cache cap failed")
			return false
		print("AUD-12 frames=", aud_12_frames, " worst_audio_ms=", aud_12_worst_ms,
			" music_starts=", bank.call("music_start_count"), " ambience_starts=", bank.call("ambience_start_count"),
			" cache_bytes=", bank.call("background_cache_bytes"), " master_peak=", aud_12_peak)
		print("aud_12_installed_walkthrough_music_starts_and_changes_under_two_ms: PASS")
		quit(0)
	return false


func _request(bank: Node, path: String) -> void:
	if not bank.call("play_music_track", path, 0.1, 0.1, false):
		_fail("music request rejected: " + str(bank.call("last_error")))
	if str(bank.call("current_music_path")) != aud_12_old_path:
		_fail("pending music replaced the current track before ready")


func _fail(message: String) -> void:
	aud_12_failures += 1
	printerr("AUD-12 FAIL: ", message)
	var bank := player.get_node("SoundBank")
	print("AUD-12 diagnostic stage=", aud_12_stage, " current=", bank.call("current_music_path"), " music_starts=", bank.call("music_start_count"), " ambience_starts=", bank.call("ambience_start_count"))
	print("AUD-12 frames=", aud_12_frames, " worst_audio_ms=", aud_12_worst_ms)
	print("aud_12_installed_walkthrough_music_starts_and_changes_under_two_ms: FAIL")
	quit(1)
