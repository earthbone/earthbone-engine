extends SceneTree

## Exercise EffectShader instance synchronization in the live RenderingServer.
var failures := 0
var exited_effects := -1

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _record_exit_cleanup(count: int) -> void:
	exited_effects = count

func _run() -> void:
	if not ClassDB.class_exists("CellStreamerNode"):
		printerr("FAIL: CellStreamerNode is not registered")
		quit(1)
		return
	var host: Node3D = ClassDB.instantiate("CellStreamerNode")
	host.connect("scr_24_effect_shaders_released", _record_exit_cleanup)
	get_root().add_child(host)
	await process_frame
	_check(host.call("scr_24_effect_shader_sync", 1, 2.0, 0.0) == 1,
		"Play did not create one RenderingServer instance")
	_check(host.call("scr_24_effect_shader_sync", 1, 1.0, 0.5) == 1,
		"repeated Play did not refresh the shader-target instance")
	_check(host.call("scr_24_effect_shader_sync", 0, 0.0, 0.4) == 1,
		"refreshed finite instance expired too early")
	_check(host.call("scr_24_effect_shader_sync", 0, 0.0, 0.2) == 0,
		"finite instance did not expire after its refreshed lifetime")
	_check(host.call("scr_24_effect_shader_sync", 1, -1.0, 0.0) == 1,
		"negative duration did not create an indefinite instance")
	_check(host.call("scr_24_effect_shader_sync", 2, 0.0, 0.0) == 0,
		"Stop did not free the keyed RenderingServer instance")
	_check(host.call("scr_24_effect_shader_sync", 1, -1.0, 0.0) == 1,
		"second Play did not create an instance")
	host.call("clear")
	_check(host.call("scr_24_effect_shader_count") == 0,
		"clear did not release the active shader instance")
	host.call("scr_24_effect_shader_sync", 1, -1.0, 0.0)
	_check(host.call("scr_24_effect_shader_count") == 1,
		"tree-exit setup did not create an active shader instance")
	host.free()
	_check(exited_effects == 1, "tree exit did not release its active shader instance")
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " scr_24 renderer lifetime sync")
	quit(0 if failures == 0 else 1)
