extends SceneTree

## RND-05 acceptance: render the installed vanilla skinned body and inspect
## the captured pixels for the authored diffuse texture's color variation.
## Run windowed with SKYRIM_DATA set and the host extension loaded.

const BODY := "meshes/actors/character/character assets/malebody_1.nif"
const BACKGROUND := Color(0.06, 0.07, 0.09)

var failures := 0


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		printerr("RESULT: FAIL: SKYRIM_DATA is not set")
		quit(2)
		return
	if not ClassDB.class_exists("SkinnedActorNode"):
		printerr("RESULT: FAIL: SkinnedActorNode is not registered")
		quit(2)
		return

	var scene := Node3D.new()
	scene.name = "RND05"
	root.add_child(scene)

	var actor := ClassDB.instantiate("SkinnedActorNode") as Node3D
	_check(actor != null, "SkinnedActorNode can be instantiated")
	if actor == null:
		quit(1)
		return
	scene.add_child(actor)
	_check(actor.call("load_character", data, BODY), "vanilla skinned body loads")
	if failures > 0:
		quit(1)
		return

	var light := DirectionalLight3D.new()
	light.rotation_degrees = Vector3(-35.0, -40.0, 0.0)
	light.light_energy = 1.3
	scene.add_child(light)

	var world := WorldEnvironment.new()
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = BACKGROUND
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color(0.7, 0.7, 0.75)
	environment.ambient_light_energy = 0.7
	world.environment = environment
	scene.add_child(world)

	var camera := Camera3D.new()
	scene.add_child(camera)
	camera.look_at_from_position(Vector3(0.0, 1.0, 2.6), Vector3(0.0, 0.9, 0.0), Vector3.UP)
	camera.current = true

	for frame in range(8):
		await process_frame
	var image := root.get_texture().get_image()
	_check(image != null and not image.is_empty(), "window capture contains rendered pixels")
	if image == null or image.is_empty():
		quit(1)
		return

	var rendered_pixels := 0
	var colored_pixels := 0
	var color_bins := {}
	for y in range(image.get_height()):
		for x in range(image.get_width()):
			var color := image.get_pixel(x, y)
			if absf(color.r - BACKGROUND.r) < 0.05 and absf(color.g - BACKGROUND.g) < 0.05 and absf(color.b - BACKGROUND.b) < 0.05:
				continue
			rendered_pixels += 1
			var high := maxf(color.r, maxf(color.g, color.b))
			var low := minf(color.r, minf(color.g, color.b))
			if high - low > 0.04:
				colored_pixels += 1
				var key := "%d,%d,%d" % [int(color.r * 8.0), int(color.g * 8.0), int(color.b * 8.0)]
				color_bins[key] = true

	print("capture: %dx%d, %d rendered character pixels, %d colored, %d color bins"
		% [image.get_width(), image.get_height(), rendered_pixels, colored_pixels, color_bins.size()])
	_check(rendered_pixels > 1000, "the vanilla character occupies the rendered frame")
	_check(colored_pixels > 100, "rendered character pixels carry diffuse color")
	_check(color_bins.size() > 6, "rendered character shows authored texture variation")
	var capture_path := "/tmp/rnd_05_vanilla_npc_material_check.png"
	var save_error := image.save_png(capture_path)
	_check(save_error == OK, "capture is saved for review at " + capture_path)

	if failures == 0:
		print("RESULT: PASS: vanilla skinned character renders its authored texture")
		quit(0)
	else:
		printerr("RESULT: FAIL: %d check(s) failed" % failures)
		quit(1)
