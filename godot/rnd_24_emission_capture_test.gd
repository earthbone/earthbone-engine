extends SceneTree

## Installed Bannered Mare glow-map capture. Bright coverage must remain below
## half of the visible chandelier while at least one candle region emits.
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
		print("SKYRIM_DATA unset, RND-24 installed capture skipped")
		quit(0)
		return
	if DisplayServer.get_name() == "headless":
		printerr("RND-24 capture needs a renderer")
		quit(1)
		return
	var viewport := SubViewport.new()
	viewport.size = Vector2i(256, 256)
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	viewport.own_world_3d = true
	get_root().add_child(viewport)
	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.far = 10000.0
	viewport.add_child(camera)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color.BLACK
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_DISABLED
	environment.environment.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	environment.environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
	viewport.add_child(environment)
	var host := CellStreamerNode.new()
	viewport.add_child(host)
	_check(host.load_data(data), "installed data opens")
	if failures > 0:
		quit(1)
		return
	var selected := "clutter/candles/candlehornchandelier01.nif"
	var meshes: Array = host.model_meshes(selected)
	_check(not meshes.is_empty(), "Bannered Mare chandelier model resolves")
	if meshes.is_empty():
		quit(1)
		return
	var visible := Node3D.new()
	viewport.add_child(visible)
	var bounds: AABB = meshes[0].get_aabb()
	for mesh in meshes:
		var instance := MeshInstance3D.new()
		instance.mesh = mesh
		visible.add_child(instance)
		bounds = bounds.merge(mesh.get_aabb())
	var center := bounds.get_center()
	var span := maxf(bounds.size.x, maxf(bounds.size.y, bounds.size.z))
	camera.size = maxf(span * 1.35, 1.0)
	camera.position = center + Vector3(span * 1.8, -span * 2.4, span * 1.8)
	camera.look_at(center, Vector3.UP)
	for frame in range(5):
		await process_frame
		RenderingServer.force_draw(false)
	var image := viewport.get_texture().get_image()
	var visible_pixels := 0
	var bright_pixels := 0
	for y in range(image.get_height()):
		for x in range(image.get_width()):
			var pixel := image.get_pixel(x, y)
			var level := maxf(pixel.r, maxf(pixel.g, pixel.b))
			if level > 0.01:
				visible_pixels += 1
			if level > 0.2:
				bright_pixels += 1
	_check(visible_pixels > 20, "chandelier has visible pixels")
	_check(bright_pixels > 0, "candle glow map produces bright pixels")
	_check(bright_pixels * 2 < visible_pixels, "glow map keeps most chandelier pixels dark")
	var capture_path := ProjectSettings.globalize_path("res://../target/rnd_24_bannered_mare_chandelier.png")
	var output := FileAccess.open(capture_path, FileAccess.WRITE)
	if output:
		output.store_buffer(image.save_png_to_buffer())
		output.close()
	print("rnd_24_installed_bannered_mare_chandelier_glow_is_masked model=", selected,
		" visible=", visible_pixels, " bright=", bright_pixels)
	host.free()
	viewport.free()
	quit(0 if failures == 0 else 1)
