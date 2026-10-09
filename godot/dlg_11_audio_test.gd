extends SceneTree

## Runs SoundBank voice assets through Godot's 3D player and captures its mixer.
var failures := 0
var finished := false
const MIN_PEAK := 0.0001
var _listener: AudioListener3D
var _camera: Camera3D
var _bank: Node3D
var _capture: AudioEffectCapture

func _initialize() -> void:
	call_deferred("_watchdog")
	call_deferred("_run")

func _watchdog() -> void:
	await create_timer(90.0).timeout
	if not finished:
		printerr("FAIL: dialogue audio test exceeded its 90 second script limit")
		_finish(1)

func _finish(exit_code: int) -> void:
	if finished:
		return
	finished = true
	if is_instance_valid(_bank):
		for player: Node in _bank.get_children():
			if player is AudioStreamPlayer3D:
				player.stop()
		_bank.free()
	if is_instance_valid(_listener):
		_listener.free()
	if is_instance_valid(_camera):
		_camera.free()
	if is_instance_valid(_capture):
		for index in range(AudioServer.get_bus_effect_count(0)):
			if AudioServer.get_bus_effect(0, index) == _capture:
				AudioServer.remove_bus_effect(0, index)
				break
	quit(exit_code)

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

func _wait_for_peak(capture: AudioEffectCapture, player: AudioStreamPlayer3D) -> float:
	var deadline := Time.get_ticks_msec() + ceili((player.stream.get_length() + 1.0) * 1000.0)
	var peak := 0.0
	while Time.get_ticks_msec() < deadline:
		await create_timer(0.05).timeout
		peak = maxf(peak, _captured_peak(capture))
		if peak > MIN_PEAK or not player.playing:
			break
	return peak

func _run() -> void:
	# CONTEXT: A fresh checkout has no editor-generated extension discovery cache.
	if not GDExtensionManager.is_extension_loaded("res://soe.gdextension"):
		GDExtensionManager.load_extension("res://soe.gdextension")
	if not ClassDB.class_exists("SoundBank"):
		printerr("FAIL: SoundBank is not registered, the extension did not load")
		_finish(1)
		return
	var data := OS.get_environment("DLG_11_TEST_DATA")
	var first_path := OS.get_environment("DLG_11_FIRST_VOICE")
	var second_path := OS.get_environment("DLG_11_SECOND_VOICE")
	if data.is_empty() or first_path.is_empty() or second_path.is_empty():
		printerr("FAIL: dialogue voice fixtures were not supplied")
		_finish(1)
		return

	_listener = AudioListener3D.new()
	get_root().add_child(_listener)
	_listener.make_current()
	_camera = Camera3D.new()
	get_root().add_child(_camera)
	_camera.make_current()
	_bank = ClassDB.instantiate("SoundBank") as Node3D
	get_root().add_child(_bank)
	for method in ["open_data", "play_voice", "stop_voice", "last_error"]:
		if not _bank.has_method(method):
			printerr("FAIL: SoundBank lacks ", method, ", rebuild the release extension")
			_finish(1)
			return
	await process_frame
	_check(_bank.call("open_data", data), "SoundBank could not open the voice fixture archive: " + str(_bank.call("last_error")))
	if failures != 0:
		_finish(1)
		return

	_capture = AudioEffectCapture.new()
	_capture.buffer_length = 2.0
	AudioServer.add_bus_effect(0, _capture)
	await process_frame
	var first_position := Vector3(1.25, 0.0, -1.0)
	var second_position := Vector3(2.0, 0.0, -1.0)
	var first_slot: int = _bank.call("play_voice", first_path, first_position)
	_check(first_slot >= 0, "first resolved voice did not start: " + str(_bank.call("last_error")))
	if first_slot < 0:
		_finish(1)
		return
	var first_player := _bank.get_child(first_slot) as AudioStreamPlayer3D
	_check(first_player.playing, "first AudioStreamPlayer3D is not playing")
	_check(first_player.position.is_equal_approx(first_position), "first voice is not at the speaker position")
	var first_peak: float = await _wait_for_peak(_capture, first_player)
	_check(first_peak > MIN_PEAK, "first voice produced no Godot mixer output")

	_check(_bank.call("stop_voice", first_slot), "interrupt did not stop the active voice")
	_check(not first_player.playing, "interrupted AudioStreamPlayer3D kept playing")
	await create_timer(0.05).timeout
	_capture.clear_buffer()
	await create_timer(0.1).timeout
	_capture.clear_buffer()
	var second_slot: int = _bank.call("play_voice", second_path, second_position)
	_check(second_slot >= 0, "replacement resolved voice did not start: " + str(_bank.call("last_error")))
	if second_slot < 0:
		_finish(1)
		return
	var second_player := _bank.get_child(second_slot) as AudioStreamPlayer3D
	_check(second_player.playing, "replacement AudioStreamPlayer3D is not playing")
	_check(second_player.position.is_equal_approx(second_position), "replacement voice is not at the speaker position")
	var second_peak: float = await _wait_for_peak(_capture, second_player)
	_check(second_peak > MIN_PEAK, "replacement voice produced no Godot mixer output")
	_check(_bank.call("stop_voice", second_slot), "cleanup did not stop the replacement voice")
	print("DLG-11 mixer peaks: first=", first_peak, " replacement=", second_peak)
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " dialogue audio playback")
	_finish(0 if failures == 0 else 1)
