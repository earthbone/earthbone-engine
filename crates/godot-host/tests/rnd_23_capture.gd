extends SceneTree

## RND-23 production RID capture. Fixed before the first run: uploaded
## transforms differ by > 0.00001, >= 100 visible pixels, >= 20 changed pixels.
var failures := 0
var viewport: SubViewport
var host: CellStreamerNode

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _capture() -> Image:
	for frame in range(4):
		await process_frame
		RenderingServer.force_draw(false)
	return viewport.get_texture().get_image()

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("rnd_23_banner_capture: SKYRIM_DATA unset, skipped")
		quit(0)
		return
	if DisplayServer.get_name() == "headless":
		printerr("rnd_23_banner_capture needs rendering")
		quit(1)
		return
	viewport = SubViewport.new()
	viewport.size = Vector2i(256, 256)
	viewport.own_world_3d = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	get_root().add_child(viewport)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color.BLACK
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color.WHITE
	environment.environment.ambient_light_energy = 1.0
	viewport.add_child(environment)
	host = CellStreamerNode.new()
	viewport.add_child(host)
	host.set_process(false)
	_check(host.load_data(data), "installed data opens")
	var model := OS.get_environment("SOE_RND23_MODEL")
	if model.is_empty():
		model = "architecture/whiterun/wrclutter/citybannerwhiterun01inside.nif"
	if model.is_empty() or host.stream_animated_model(model) == 0:
		printerr("FAIL: banner has no live surfaces")
		quit(1)
		return
	var bounds := host.scene_bounds()
	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = maxf(bounds.size.length(), 1.0) * 1.15
	camera.far = 10000.0
	viewport.add_child(camera)
	var center := bounds.get_center()
	camera.position = center + Vector3(1.0, 0.1, 0.25).normalized() * camera.size * 2
	camera.look_at(center)
	host.step_object_animations(0.1)
	var first_poses := host.object_animation_transforms()
	var first := await _capture()
	host.step_object_animations(0.6)
	var second_poses := host.object_animation_transforms()
	var second := await _capture()
	var pose_delta := 0.0
	for i in range(mini(first_poses.size(), second_poses.size())):
		pose_delta = maxf(pose_delta, first_poses[i].origin.distance_to(second_poses[i].origin))
		for axis in range(3):
			pose_delta = maxf(pose_delta, first_poses[i].basis[axis].distance_to(second_poses[i].basis[axis]))
	_check(pose_delta > 0.00001, "uploaded banner transforms differ: %f" % pose_delta)
	var visible := 0
	var changed := 0
	for y in range(256):
		for x in range(256):
			var a := first.get_pixel(x, y)
			var b := second.get_pixel(x, y)
			if maxf(a.r, maxf(a.g, a.b)) > 0.01:
				visible += 1
			if absf(a.r-b.r) + absf(a.g-b.g) + absf(a.b-b.b) > 0.01:
				changed += 1
	_check(visible >= 100, "banner visible pixels: %d" % visible)
	_check(changed >= 20, "banner changed pixels: %d" % changed)
	_check(first.save_png("res://../target/rnd_23_banner_0.png") == OK, "first capture saved")
	_check(second.save_png("res://../target/rnd_23_banner_1.png") == OK, "second capture saved")
	print("rnd_23_banner_capture poses=", first_poses.size(), " pose_delta=", pose_delta, " visible=", visible, " changed=", changed)
	# CONTEXT: The object path adds resources only, so it must add no scene children.
	_check(host.get_child_count() == 0, "object playback adds no per-reference nodes")
	host.clear()
	_check(host.object_animation_transforms().is_empty(), "clear releases object animation resources")
	_check(host.stream_animated_model("clutter/upperclass/upperchest01.nif") > 0, "chest streams")
	var closed := host.object_animation_transforms()
	_check(host.activate_object(0, 0), "activation selects authored Open sequence")
	host.step_object_animations(0.4)
	var opened := host.object_animation_transforms()
	_check(closed != opened, "chest activation changes uploaded poses")
	_check(host.play_object_sequence(0, 0, "Close") > 0, "named Close sequence starts")
	_check(host.play_object_sequence(0, 0, "missing") == 0, "unknown sequence is rejected")
	host.clear()
	host.free()
	viewport.free()
	print("rnd_23_banner_capture: ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
