extends SceneTree

## Check the live PlayerNode camera and its drawn strip at one-degree steps.
const STEP_ERROR_PX := 0.001
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	var directory := OS.get_environment("SOE_UI_29_FIXTURE_DIR")
	_check(player.call("debug_load_mcm_fixture", directory), "minimal game did not load")
	await process_frame
	await process_frame
	var compass := player.get_node("HudLayer/Hud/Compass") as Control
	_check(compass != null and compass.is_visible_in_tree(), "strip is invisible")
	_check(compass.clip_contents, "strip is not clipped to its viewport")
	var previous := 0.0
	var changed := 0
	for degree in range(361):
		player.call("set_facing", deg_to_rad(float(degree)), 0.0)
		var offset: float = compass.get("scroll_offset")
		if degree > 0:
			var step := fposmod(offset - previous, 720.0)
			_check(absf(step - 718.0) <= STEP_ERROR_PX, "nonlinear offset at degree %d: %f" % [degree, step])
			if not is_equal_approx(offset, previous):
				changed += 1
		previous = offset
		await process_frame
	_check(changed == 360, "some camera steps left the strip still")
	_check(str(compass.get("heading_text")).contains("[N]"), "wrap did not return to north")
	await RenderingServer.frame_post_draw
	var image := get_root().get_texture().get_image()
	var area := Rect2i(compass.get_global_rect()).intersection(Rect2i(Vector2i.ZERO, image.get_size()))
	var ink := 0
	for y in range(area.position.y, area.end.y):
		for x in range(area.position.x, area.end.x):
			if image.get_pixel(x, y).get_luminance() > 0.5:
				ink += 1
	_check(ink >= 100, "labels and ticks produced too few viewport pixels")
	_check(image.save_png(directory.path_join("compass.png")) == OK, "capture failed")
	player.free()
	await process_frame
	print("PASS ui_29 compass 361 samples" if failures == 0 else "FAIL ui_29 compass")
	quit(0 if failures == 0 else 1)
