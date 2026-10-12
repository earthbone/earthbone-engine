extends SceneTree

const HEAD_MALE := "meshes/actors/character/character assets/malehead.nif"
const HEAD_FEMALE := "meshes/actors/character/character assets/femalehead.nif"
const CELL := "WhiterunBanneredMare"

var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _capture() -> Image:
	await process_frame
	await RenderingServer.frame_post_draw
	return get_root().get_texture().get_image()

func _different_pixels(a: Image, b: Image) -> int:
	var left := a.get_data()
	var right := b.get_data()
	var changed := 0
	for index in left.size():
		if left[index] != right[index]:
			changed += 1
	return changed

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: ANM-59 requires SKYRIM_DATA and the host extension")
		quit(1)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "vanilla data load failed")
	_check(streamer.call("stream_cell", CELL) > 0, "vanilla NPC cell did not stream")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source failed")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0,
		"vanilla NPC cell did not enter")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	var npc: PackedInt64Array = player.call("debug_first_loaded_humanoid_npc")
	_check(npc.size() == 3, "loaded cell has no non-player vanilla humanoid actor")
	if npc.size() != 3:
		player.free()
		streamer.free()
		quit(1)
		return
	var head_path := HEAD_FEMALE if npc[2] != 0 else HEAD_MALE
	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	root.add_child(visual)
	_check(visual.call("load_character", data, head_path), "vanilla NPC head failed to load")
	_check(player.call("bind_actor_expression_head", visual, int(npc[0]), npc[1]),
		"vanilla NPC head did not bind to its actor state")
	var stats: Dictionary = visual.call("stats")
	_check(int(stats.get("expression_shapes", 0)) > 0,
		"vanilla NPC head has no expression blend shapes")

	var camera := Camera3D.new()
	camera.position = Vector3(0.0, 0.0, 2.5)
	camera.look_at(Vector3.ZERO)
	camera.current = true
	root.add_child(camera)
	var light := DirectionalLight3D.new()
	light.rotation_degrees = Vector3(-25.0, 25.0, 0.0)
	root.add_child(light)
	_check(player.call("debug_set_actor_expression_mood", npc[0], npc[1], 3.0),
		"vanilla NPC mood could not be set to Happy")
	visual.call("advance", 0.0)
	var baseline := await _capture()
	_check(player.call("debug_set_actor_expression_override", npc[0], npc[1], 12, 45),
		"vanilla NPC expression override could not be set")
	visual.call("advance", 0.0)
	var overridden := await _capture()
	_check(_different_pixels(baseline, overridden) > 0,
		"expression override did not change rendered head geometry")
	_check(player.call("debug_set_actor_expression_mood", npc[0], npc[1], 4.0),
		"vanilla NPC mood could not change while overridden")
	visual.call("advance", 0.0)
	var held := await _capture()
	_check(_different_pixels(overridden, held) == 0,
		"record mood changed the rendered expression before clear")
	_check(player.call("debug_clear_actor_expression_override", npc[0], npc[1]),
		"vanilla NPC expression override could not be cleared")
	visual.call("advance", 0.0)
	var resumed := await _capture()
	_check(_different_pixels(overridden, resumed) > 0,
		"rendered NPC head did not resume its changed mood after clear")
	var weights: Dictionary = visual.call("debug_expression_weights")
	_check(is_equal_approx(float(weights.get("MoodSad", -1.0)), 1.0),
		"clearing did not restore the latest MoodSad target")

	visual.free()
	camera.free()
	light.free()
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ANM-59 rendered vanilla NPC head")
	quit(0 if failures == 0 else 1)
