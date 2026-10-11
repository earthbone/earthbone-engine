extends SceneTree

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	# CONTEXT: A fresh checkout has no editor-generated extension discovery cache.
	if not GDExtensionManager.is_extension_loaded("res://soe.gdextension"):
		GDExtensionManager.load_extension("res://soe.gdextension")
	if not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: PlayerNode is not registered, the extension did not load")
		quit(1)
		return
	var data := OS.get_environment("SKYRIM_DATA")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "vanilla data did not load")
	_check(player.call("use_streamer_data", streamer), "PlayerNode did not share vanilla data")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) >= 0,
		"test cell did not load")
	_check(player.call("start_gameplay"), "shared Game did not start")
	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	get_root().add_child(visual)
	_check(visual.call("load_character", data,
		"meshes/actors/character/character assets/malebody_1.nif"), "vanilla behavior graph did not load")
	_check(player.call("bind_actor_appearance", visual, 0, 0x14),
		"player rig did not bind to its game reference")
	var event_wait: int = player.call("debug_start_animation_wait", 0, 0x14, "ANM58Signal")
	_check(event_wait >= 0, "Papyrus event wait did not start")
	_check(player.call("debug_animation_wait_result", event_wait) == -2,
		"Papyrus event wait was not suspended before the signal")
	visual.emit_signal("animation_event", "ANM58Signal")
	_check(player.call("debug_animation_wait_result", event_wait) == 1,
		"Godot animation signal did not resume the Papyrus wait true")
	var events: Array = player.call("gameplay_animation_events")
	_check(events.size() == 1, "Godot animation signal did not reach Game")
	if events.size() == 1:
		_check(events[0].get("name") == "ANM58Signal", "Game recorded the wrong animation event")
		_check(events[0].get("source_file") == 0 and events[0].get("source_object") == 0x14,
			"Game recorded the wrong animation source: %s" % [events[0]])
	var unload_wait: int = player.call("debug_start_animation_wait", 0, 0x14, "ANM58NeverFires")
	_check(unload_wait >= 0, "Papyrus unload wait did not start")
	_check(player.call("debug_animation_wait_result", unload_wait) == -2,
		"Papyrus unload wait was not suspended before graph unload")
	visual.emit_signal("animation_graph_unloaded")
	_check(player.call("debug_animation_wait_result", unload_wait) == 0,
		"Godot graph unload did not resume the Papyrus wait false")
	await process_frame
	visual.free()
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " anm_58 signal waits")
	quit(0 if failures == 0 else 1)
