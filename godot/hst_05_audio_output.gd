extends SceneTree

## Plays one short SoundBank effect three times for a physical output check.
const DATA_PATH := "sound/fx/fst/dirt/jumpdown/fst_dirt_jumpdown_01.wav"
const PLAY_GAP_SECONDS := 0.8

func _initialize() -> void:
	call_deferred("_run")

func _fail(reason: String) -> void:
	printerr("AUDIO_CHECK failed: ", reason)
	quit(1)

func _run() -> void:
	var devices := AudioServer.get_output_device_list()
	for device: String in devices:
		print("AUDIO_DEVICE name=", device)
	print("AUDIO_DEVICES count=", devices.size())

	var arguments := OS.get_cmdline_user_args()
	var requested_device := "Default"
	var sound_path := DATA_PATH
	if arguments.size() > 0 and not arguments[0].is_empty():
		requested_device = arguments[0]
	if arguments.size() > 1 and not arguments[1].is_empty():
		sound_path = arguments[1]

	if requested_device != "Default" and not devices.has(requested_device):
		_fail("audio output device was not found: " + requested_device)
		return
	AudioServer.set_output_device(requested_device)
	var selected_device := AudioServer.get_output_device()
	print("AUDIO_SELECTED device=", selected_device)
	if selected_device != requested_device:
		_fail("Godot selected " + selected_device + " instead of " + requested_device)
		return

	var data_path := OS.get_environment("SKYRIM_DATA")
	if data_path.is_empty():
		_fail("SKYRIM_DATA is not set")
		return
	if not ClassDB.class_exists("CellStreamerNode") or not ClassDB.class_exists("SoundBank"):
		_fail("the host extension is not loaded")
		return

	var camera := Camera3D.new()
	get_root().add_child(camera)
	camera.make_current()
	var streamer := ClassDB.instantiate("CellStreamerNode") as Node3D
	get_root().add_child(streamer)
	if not streamer.call("load_data", data_path):
		_fail("CellStreamerNode could not load Skyrim data")
		return
	var bank := ClassDB.instantiate("SoundBank") as Node3D
	get_root().add_child(bank)
	if not bank.call("use_streamer_data", streamer):
		_fail("SoundBank could not use the loaded game data")
		return

	for played in range(1, 4):
		var slot: int = bank.call("play_effect", sound_path, Vector3.ZERO)
		if slot < 0:
			_fail("SoundBank could not play " + sound_path + ": " + str(bank.call("last_error")))
			return
		var player := bank.get_child(slot) as AudioStreamPlayer3D
		if player == null or not player.playing:
			_fail("SoundBank did not start AudioStreamPlayer3D for " + sound_path)
			return
		print("AUDIO_CHECK device=", selected_device, " sound=", sound_path, " played=", played)
		await create_timer(PLAY_GAP_SECONDS).timeout

	quit(0)
