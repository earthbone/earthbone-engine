extends SceneTree

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	if not GDExtensionManager.is_extension_loaded("res://soe.gdextension"):
		GDExtensionManager.load_extension("res://soe.gdextension")
	if not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: PlayerNode is not registered")
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
	_check(player.call("use_streamer_data", streamer), "shared data did not bind")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) >= 0,
		"test cell did not load")
	_check(player.call("start_gameplay"), "shared Game did not start")
	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	get_root().add_child(visual)
	_check(visual.call("load_character", data,
		"meshes/actors/character/character assets/malebody_1.nif"), "vanilla graph did not load")
	_check(player.call("bind_actor_appearance", visual, 0, 0x14), "actor rig did not bind")

	var pending: int = player.call("debug_start_animation_wait", 0, 0x14, "ANM58Expected")
	_check(player.call("debug_animation_wait_result", pending) == -2, "wait was not pending")
	visual.emit_signal("animation_event", "ANM58Other")
	_check(player.call("debug_animation_wait_result", pending) == -2,
		"an unrelated event incorrectly completed the wait")
	visual.emit_signal("animation_event", "anm58expected")
	_check(player.call("debug_animation_wait_result", pending) == 1,
		"case-insensitive matching event did not return true")
	var events: Array = player.call("gameplay_animation_events")
	_check(events.size() == 2, "each signal was not recorded exactly once")
	if events.size() == 2:
		_check(events[0].get("name") == "ANM58Other" and events[1].get("name") == "anm58expected",
			"event names or order changed in Game")

	var disabled_wait: int = player.call("debug_start_animation_wait", 0, 0x14, "ANM58Disabled")
	_check(player.call("debug_animation_wait_result", disabled_wait) == -2,
		"disable wait was not pending")
	_check(player.call("set_reference_enabled", 0, 0x14, false), "reference disable failed")
	player.call("advance", 0.016667)
	_check(player.call("debug_animation_wait_result", disabled_wait) == 0,
		"disabling the reference did not return false")
	_check(player.call("set_reference_enabled", 0, 0x14, true), "reference re-enable failed")
	var unload_wait: int = player.call("debug_start_animation_wait", 0, 0x14, "ANM58Unload")
	_check(player.call("debug_animation_wait_result", unload_wait) == -2,
		"unload wait was not pending")
	visual.emit_signal("animation_graph_unloaded")
	_check(player.call("debug_animation_wait_result", unload_wait) == 0,
		"graph unload did not return false")
	visual.free()
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " anm_58 adversarial")
	quit(0 if failures == 0 else 1)
