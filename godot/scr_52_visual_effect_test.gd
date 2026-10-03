extends SceneTree

## Installed-data capture of a World-owned RFCT effect rendered by the host.
## Run windowed with --resolution 640x360 and the extension built.

var failures := 0


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: scr_52_visual_effect_test requires SKYRIM_DATA and the extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	_check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0, "cell render failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"player collision failed")
	_check(player.call("start_gameplay"), "game runtime failed to start")
	var effects: Array = player.call("gameplay_visual_effect_forms")
	_check(not effects.is_empty(), "installed data has no RFCT with ARTO or EFSH")
	if not effects.is_empty():
		var selected: Dictionary = effects[0]
		var has_camera_attachment := false
		for candidate: Dictionary in effects:
			if (int(candidate.flags) & 0x02) != 0:
				selected = candidate
				has_camera_attachment = true
				break
		_check(has_camera_attachment, "installed data has no camera-attached RFCT with art or shader")
		var pose: Dictionary = player.call("gameplay_reference_pose", 0, 0x14)
		var skyrim: Vector3 = pose.position
		var center := Vector3(skyrim.x * 0.0142875, skyrim.z * 0.0142875,
			-skyrim.y * 0.0142875)
		var camera := Camera3D.new()
		get_root().add_child(camera)
		camera.look_at_from_position(center + Vector3(0.0, 1.5, 4.0), center, Vector3.UP)
		camera.current = true
		await process_frame
		await process_frame
		var before_effect: Image = get_root().get_texture().get_image()
		# The reserved player reference is the visible host character in this cell.
		_check(player.call("gameplay_play_visual_effect", selected.file, selected.object,
			0, 0x14, 10.0), "World rejected the installed RFCT effect")
		for frame in range(3):
			player.call("advance", 1.0 / 60.0)
		_check(streamer.call("visual_effect_render_count") == 1,
			"active World RFCT effect did not create a host render instance")
		await process_frame
		await process_frame
		var sample_effect: Image = get_root().get_texture().get_image()
		var changed_pixels := 0
		for y in range(120, 241):
			for x in range(200, 441):
				var before_pixel := before_effect.get_pixel(x, y)
				var effect_pixel := sample_effect.get_pixel(x, y)
				var delta: float = (
					abs(before_pixel.r - effect_pixel.r) +
					abs(before_pixel.g - effect_pixel.g) +
					abs(before_pixel.b - effect_pixel.b)
				)
				if delta > 0.08:
					changed_pixels += 1
		_check(changed_pixels > 20, "RFCT host visual is absent from the captured view")
		var capture := "/tmp/scr_52_visual_effect.png"
		_check(sample_effect.save_png(capture) == OK, "could not save host visual capture")
		var stopped: int = player.call("gameplay_stop_visual_effect", selected.file,
			selected.object, 0, 0x14)
		_check(stopped == 1, "World Stop did not remove one effect-target pair")
		player.call("advance", 1.0 / 60.0)
		_check(streamer.call("visual_effect_render_count") == 0,
			"Stop left a host render instance alive")
		print("SCR-52 RFCT capture: ", capture, " form=", selected.file, ":", selected.object,
			" shader=", selected.shader, " flags=", selected.flags)
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " scr_52_visual_effect")
	quit(0 if failures == 0 else 1)
