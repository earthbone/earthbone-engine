extends SceneTree

const HEAD_MALE := "meshes/actors/character/character assets/malehead.nif"
const HEAD_FEMALE := "meshes/actors/character/character assets/femalehead.nif"
const CELL := "WhiterunBanneredMare"

var failures := 0
var head_view: SubViewport

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _capture() -> Image:
	await process_frame
	await RenderingServer.frame_post_draw
	return head_view.get_texture().get_image()

func _different_pixels(a: Image, b: Image) -> int:
	var left := a.get_data()
	var right := b.get_data()
	var changed := 0
	for index in left.size():
		if left[index] != right[index]:
			changed += 1
	return changed

func _posed_bounds(observation: Dictionary) -> AABB:
	var bounds := AABB()
	var first := true
	var poses: Array = observation.poses
	var bone_offset := 0
	for shape: Dictionary in observation.shapes:
		var mesh := shape.mesh as ArrayMesh
		var arrays := mesh.surface_get_arrays(0)
		var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
		var bones: PackedInt32Array = arrays[Mesh.ARRAY_BONES]
		var weights: PackedFloat32Array = arrays[Mesh.ARRAY_WEIGHTS]
		var bone_count := 0
		for bone: int in bones:
			bone_count = maxi(bone_count, bone + 1)
		for vertex in vertices.size():
			var point := Vector3.ZERO
			for influence in 4:
				var index := vertex * 4 + influence
				var pose: Transform3D = poses[bone_offset + bones[index]]
				point += (pose * vertices[vertex]) * weights[index]
			bounds = AABB(point, Vector3.ZERO) if first else bounds.expand(point)
			first = false
		bone_offset += bone_count
	return bounds

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
	head_view = SubViewport.new()
	head_view.size = Vector2i(640, 360)
	head_view.own_world_3d = true
	head_view.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(head_view)
	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	head_view.add_child(visual)
	_check(visual.call("load_character", data, head_path), "vanilla NPC head failed to load")
	visual.set("playing", false)
	_check(player.call("bind_actor_expression_head", visual, int(npc[0]), npc[1]),
		"vanilla NPC head did not bind to its actor state")
	visual.call("advance", 0.0)
	var stats: Dictionary = visual.call("stats")
	_check(int(stats.get("expression_shapes", 0)) > 0,
		"vanilla NPC head has no expression blend shapes")

	var observation: Dictionary = visual.call("play_path_observation")
	var bounds := _posed_bounds(observation)
	_check(bounds.size.length() > 0.0, "uploaded NPC head has nonempty bounds")
	var center := bounds.get_center()
	var camera := Camera3D.new()
	camera.position = center - Vector3(0.0, 0.0, maxf(0.5, bounds.size.length() * 3.0))
	camera.current = true
	head_view.add_child(camera)
	camera.look_at(center)
	var light := DirectionalLight3D.new()
	light.rotation_degrees = Vector3(-25.0, 25.0, 0.0)
	head_view.add_child(light)
	_check(player.call("debug_set_actor_expression_mood", npc[0], npc[1], 3.0),
		"vanilla NPC mood could not be set to Happy")
	visual.call("advance", 0.0)
	var baseline := await _capture()
	var target := OS.get_environment("CARGO_TARGET_DIR")
	baseline.save_png(target.path_join("anm_59_head_baseline.png"))
	print("ANM-59 head bounds=", bounds, " camera=", camera.position)
	_check(player.call("debug_set_actor_expression_override", npc[0], npc[1], 12, 45),
		"vanilla NPC expression override could not be set")
	visual.call("advance", 0.0)
	var overridden := await _capture()
	overridden.save_png(target.path_join("anm_59_head_override.png"))
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
	head_view.free()
	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ANM-59 rendered vanilla NPC head")
	quit(0 if failures == 0 else 1)
