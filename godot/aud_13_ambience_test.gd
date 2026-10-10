extends SceneTree

## AUD-13 uses one-second fixture loops to expose premature release.
var failures := 0

func _initialize() -> void:
	create_timer(180.0).timeout.connect(func():
		printerr("FAIL: AUD-13 scene exceeded 180 seconds")
		quit(1))
	call_deferred("aud_13_scene_crossfades_acoustic_and_regional_ambience")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _wait(bank: Node, count: int) -> void:
	var started := Time.get_ticks_msec()
	while int(bank.call("ambience_start_count")) < count and Time.get_ticks_msec() - started < 5000:
		await process_frame
	_check(int(bank.call("ambience_start_count")) == count, "ambience start count " + str(count))

func _voices(bank: Node) -> Array[AudioStreamPlayer]:
	var result: Array[AudioStreamPlayer] = []
	for child: Node in bank.get_children():
		if child is AudioStreamPlayer and child.has_meta("descriptor"):
			result.append(child)
	return result

func aud_13_scene_crossfades_acoustic_and_regional_ambience() -> void:
	var streamer: Node = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "fixture load")
	var audio_root := Node3D.new()
	root.add_child(audio_root)
	var bank: Node = ClassDB.instantiate("SoundBank")
	audio_root.add_child(bank)
	_check(bank.call("use_streamer_data", streamer), "shared source")
	_check(bank.call("preview_cell_ambience", 0, 0x800, 1, 0.25), "interior selection")
	await _wait(bank, 1)
	await create_timer(1.2).timeout
	var old := _voices(bank)[0]
	_check(old.playing, "short interior stream loops")
	_check(str(bank.call("acoustic_space")).ends_with("000900"), "CELL acoustic space identity")
	_check(bank.call("preview_cell_ambience", 0, 0x801, 1, 0.25), "second acoustic selection")
	_check(old.playing, "outgoing voice retained during decode")
	await _wait(bank, 2)
	await create_timer(0.25).timeout
	_check(int(bank.call("ambience_playing_count")) == 2, "crossfade overlaps voices")
	_check(old.volume_db < -0.1 and old.volume_db > -40, "outgoing gain ramps")
	await create_timer(1.1).timeout
	_check(_voices(bank).size() == 1, "outgoing node is freed after fade")
	_check(str(bank.call("acoustic_space")).ends_with("000901"), "new acoustic identity")
	_check(bank.call("preview_cell_ambience", 0, 0x802, 1, 0.25), "exterior pleasant selection")
	_check(bank.call("preview_cell_ambience", 0, 0x802, 2, 0.25), "latest cloudy selection")
	await _wait(bank, 3)
	await create_timer(1.2).timeout
	_check(_voices(bank).size() == 1, "superseded region decode cannot start")
	_check(str(_voices(bank)[0].get_meta("descriptor")).ends_with("000A03"), "weather mask chose cloudy descriptor")
	_check(str(bank.call("acoustic_space")).is_empty(), "exterior clears interior acoustic space")
	_check(bank.call("preview_cell_ambience", 0, 0x804, 1, 0.25), "missing replacement queued")
	await create_timer(0.5).timeout
	_check(int(bank.call("ambience_playing_count")) == 1, "failed decode preserves current audio")
	_check(not str(bank.call("last_error")).is_empty(), "failed decode reports asset path")
	streamer.free()
	_check(bank.get_parent() == audio_root and int(bank.call("ambience_playing_count")) == 1, "audio survives streamer destruction")
	_check(not bank.call("preview_cell_ambience", 0, 0xFFFF, 1, 0.25), "missing cell rejects input")
	_check(bank.call("preview_cell_ambience", 0, 0x803, 1, 0.25), "empty destination selection")
	await create_timer(1.2).timeout
	_check(int(bank.call("ambience_playing_count")) == 0 and _voices(bank).is_empty(), "empty destination fades and releases voices")
	_check(bank.call("play_music_track", "sound/a.wav", 0.0, 0.5, false), "first music request")
	var music_wait := Time.get_ticks_msec()
	while int(bank.call("music_start_count")) < 1 and Time.get_ticks_msec() - music_wait < 5000:
		await process_frame
	_check(int(bank.call("music_start_count")) == 1, "first music decode")
	_check(bank.call("play_music_track", "sound/b.wav", 0.5, 0.5, false), "second music request")
	music_wait = Time.get_ticks_msec()
	while int(bank.call("music_start_count")) < 2 and Time.get_ticks_msec() - music_wait < 5000:
		await process_frame
	await create_timer(0.2).timeout
	_check(int(bank.call("music_playing_count")) == 2, "music crossfade overlaps players")
	var first := bank.get_node("MusicPlayer0") as AudioStreamPlayer
	var second := bank.get_node("MusicPlayer1") as AudioStreamPlayer
	_check(first.volume_db < -0.1 and second.volume_db < -0.1, "both music gains ramp")
	await create_timer(1.1).timeout
	_check(int(bank.call("music_playing_count")) == 0, "one-second tracks finished")
	var decoded := int(bank.call("decoded_count"))
	_check(bank.call("play_music_track", "sound/b.wav", 0.0, 0.0, false), "ended track restart")
	_check(int(bank.call("music_start_count")) == 3 and int(bank.call("music_playing_count")) == 1, "ended cached music restarts")
	_check(int(bank.call("decoded_count")) == decoded, "music restart reused decode")
	audio_root.free()
	print("AUD-13 RESULT: ", "PASS" if failures == 0 else "FAIL", " fixture crossfades")
	quit(0 if failures == 0 else 1)
