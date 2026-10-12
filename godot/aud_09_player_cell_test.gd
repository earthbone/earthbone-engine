extends SceneTree

## Drives a loaded PlayerNode through two vanilla cells and inspects ambience ownership.

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
		printerr("SKYRIM_DATA is required")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "streamer did not load game data")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "player did not share game data")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"player did not enter the source cell")
	_check(player.call("start_gameplay"), "gameplay did not start")
	var bank := player.get_node_or_null("SoundBank")
	_check(bank != null, "PlayerNode did not attach its SoundBank")
	if bank == null:
		quit(1)
		return
	var initial_starts := int(bank.call("ambience_start_count"))
	_check(initial_starts > 0, "source cell selected no playable ambience descriptors")
	var departed_players: Array[AudioStreamPlayer] = []
	for child in bank.get_children():
		if child is AudioStreamPlayer and child.playing:
			departed_players.append(child)
	_check(not departed_players.is_empty(), "source cell has no active ambience player")
	_check(player.call("enter_cell", "WhiterunOrigin", 0.0, 0.0, 500.0) > 0,
		"player did not change to the destination cell")
	await process_frame
	_check(int(bank.call("ambience_start_count")) > initial_starts,
		"destination cell did not start its eligible ambience")
	for old_player in departed_players:
		_check(not is_instance_valid(old_player) or not old_player.playing,
			"departed cell ambience kept playing")
	print("AUD-09 PlayerNode cell transition: source_starts=", initial_starts,
		" destination_starts=", bank.call("ambience_start_count"))
	player.queue_free()
	streamer.queue_free()
	await process_frame
	print("AUD-09 RESULT: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
