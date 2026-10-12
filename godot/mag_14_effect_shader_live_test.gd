extends SceneTree

## Exercise MAG-14 Play and Stop through the live RenderingServer path.
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	if not ClassDB.class_exists("CellStreamerNode"):
		printerr("FAIL: CellStreamerNode is not registered")
		quit(1)
		return
	var host: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(host)
	await process_frame
	_check(host.call("scr_24_effect_shader_sync", 1, -1.0, 0.0) == 1,
		"Play did not create one host shader instance")
	_check(host.call("scr_24_effect_shader_count") == 1,
		"Play did not retain one keyed host shader instance")
	_check(host.call("scr_24_effect_shader_sync", 2, 0.0, 0.0) == 0,
		"Stop did not release the host shader instance")
	_check(host.call("scr_24_effect_shader_count") == 0,
		"Stop left a host shader instance active")
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " mag_14_effect_shader_live")
	quit(0 if failures == 0 else 1)
