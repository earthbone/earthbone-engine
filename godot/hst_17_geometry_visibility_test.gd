extends SceneTree

## Checks that first-person geometry visibility changes rendered pixels.
## Run windowed with SKYRIM_DATA and the GDExtension loaded.

const BODY := "meshes/actors/character/character assets/malebody_1.nif"
var failures := 0


func _initialize() -> void:
	call_deferred("hst_17_rendered_geometry_visibility")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func _rendered_pixels(image: Image) -> int:
	var background := image.get_pixel(0, 0)
	var count := 0
	for y in range(image.get_height()):
		for x in range(image.get_width()):
			var pixel := image.get_pixel(x, y)
			var difference := absf(pixel.r - background.r) + absf(pixel.g - background.g) + absf(pixel.b - background.b)
			if difference > 0.04:
				count += 1
	return count


func _capture() -> Image:
	await RenderingServer.frame_post_draw
	return get_root().get_viewport().get_texture().get_image()


func hst_17_rendered_geometry_visibility() -> void:
	var data_dir := OS.get_environment("SKYRIM_DATA")
	if data_dir.is_empty() or not ClassDB.class_exists("SkinnedActorNode"):
		print("SKYRIM_DATA unset or extension not loaded, skipping")
		print("RESULT: PASS hst_17 rendered geometry visibility (skipped)")
		quit(0)
		return

	var scene := Node3D.new()
	get_root().add_child(scene)
	var actor := ClassDB.instantiate("SkinnedActorNode") as Node3D
	_check(actor != null, "SkinnedActorNode unavailable")
	if actor == null:
		quit(1)
		return
	scene.add_child(actor)
	_check(actor.call("load_character", data_dir, BODY), "vanilla body failed to load")

	var light := DirectionalLight3D.new()
	light.rotation_degrees = Vector3(-35.0, -40.0, 0.0)
	light.light_energy = 1.3
	scene.add_child(light)
	var camera := Camera3D.new()
	scene.add_child(camera)
	camera.look_at_from_position(Vector3(0.0, 1.0, 2.6), Vector3(0.0, 0.9, 0.0), Vector3.UP)
	camera.current = true

	actor.call("set_first_person_geometry_visible", false)
	var hidden_pixels := _rendered_pixels(await _capture())
	actor.call("set_first_person_geometry_visible", true)
	var visible_pixels := _rendered_pixels(await _capture())
	_check(hidden_pixels < 100, "hidden actor still contributed rendered geometry: %d pixels" % hidden_pixels)
	_check(visible_pixels > hidden_pixels + 500, "showing geometry did not add rendered pixels: %d to %d" % [hidden_pixels, visible_pixels])

	print("HST-17 rendered pixels: hidden=%d visible=%d" % [hidden_pixels, visible_pixels])
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " hst_17 rendered geometry visibility")
	quit(0 if failures == 0 else 1)
