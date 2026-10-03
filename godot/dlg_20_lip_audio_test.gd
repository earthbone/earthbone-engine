extends SceneTree

## Drives LIP sampling and interruption through a live SoundBank node.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _weights(bank: Node) -> PackedFloat32Array:
	return bank.call("dialogue_mouth_weights")

func _run() -> void:
	var extension_status := GDExtensionManager.load_extension("res://soe.gdextension")
	if extension_status != GDExtensionManager.LOAD_STATUS_OK and extension_status != GDExtensionManager.LOAD_STATUS_ALREADY_LOADED:
		printerr("FAIL: could not load res://soe.gdextension, status ", extension_status)
		quit(1)
		return
	if not ClassDB.class_exists("SoundBank"):
		printerr("FAIL: SoundBank is not registered, the extension did not load")
		quit(1)
		return
	var data := OS.get_environment("DLG_20_TEST_DATA")
	var voice := OS.get_environment("DLG_20_VOICE")
	if data.is_empty() or voice.is_empty():
		printerr("FAIL: DLG-20 voice fixture was not supplied")
		quit(1)
		return

	var camera := Camera3D.new()
	camera.current = true
	get_root().add_child(camera)
	var capture := AudioEffectCapture.new()
	capture.buffer_length = 2.0
	AudioServer.add_bus_effect(0, capture)
	var bank := ClassDB.instantiate("SoundBank") as Node3D
	get_root().add_child(bank)
	await process_frame
	_check(bank.call("open_data", data), "SoundBank could not open the voice fixture archive: " + str(bank.call("last_error")))
	if failures != 0:
		quit(1)
		return

	var slot: int = bank.call("play_voice", voice, Vector3.ZERO)
	_check(slot >= 0, "voice with native LIP did not start: " + str(bank.call("last_error")))
	if slot < 0:
		quit(1)
		return
	var player := bank.get_child(slot) as AudioStreamPlayer3D
	_check(player.playing, "SoundBank voice player is not playing")
	player.play(0.55)
	var sampled := false
	var sampled_position := 0.0
	var lip_frames: Array[Vector2] = [Vector2(0.1, 0.2), Vector2(0.3, 0.4), Vector2(0.5, 0.6), Vector2(0.7, 0.8)]
	for _frame in range(20):
		await process_frame
		sampled_position = player.get_playback_position()
		var expected_frame := mini(3, int(floor(sampled_position * 4.0)))
		var expected: Vector2 = lip_frames[expected_frame]
		var actual := _weights(bank)
		if actual.size() == 2 and is_equal_approx(actual[0], expected.x) and is_equal_approx(actual[1], expected.y):
			sampled = true
			break
	_check(sampled_position > 0.0, "Godot playback position did not advance")
	_check(sampled, "mouth weights did not match the LIP frame for the voice playback time")
	_check(_weights(bank).size() == 2, "decoded LIP target count was not exposed")
	_check(bank.call("stop_voice", slot), "interrupt did not stop the active voice")
	var reset := _weights(bank)
	_check(reset.size() == 2, "interruption discarded the neutral mouth target shape")
	if reset.size() == 2:
		_check(is_zero_approx(reset[0]) and is_zero_approx(reset[1]), "interruption did not reset mouth weights")
	await process_frame
	var after_stop := _weights(bank)
	if after_stop.size() == 2:
		_check(is_zero_approx(after_stop[0]) and is_zero_approx(after_stop[1]), "interrupted voice updated mouth weights again")
	bank.queue_free()
	AudioServer.remove_bus_effect(0, AudioServer.get_bus_effect_count(0) - 1)
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " dialogue LIP playback")
	quit(0 if failures == 0 else 1)
