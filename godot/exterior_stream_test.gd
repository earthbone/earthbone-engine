extends SceneTree

## A generated flat LAND grid exercises walking, column loading and hysteresis.
const STEP := 1.0 / 60.0
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _cell(x: int, y: int) -> String:
	return "0:%06X" % (0x300 + (y + 2) * 8 + x + 2)

func _run() -> void:
	var data := OS.get_environment("SOE_EXTERIOR_FIXTURE")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: SOE_EXTERIOR_FIXTURE and extension required")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "fixture load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared source failed")
	_check(player.call("enter_cell", "Grid0_0", 4090.0, 2048.0, 120.0) >= 0, "entry failed")
	_check(player.call("start_gameplay"), "runtime start failed")
	for i in range(120):
		player.call("advance", STEP)
	_check(player.call("is_on_ground"), "initial LAND has no supporting collision")
	var original: Dictionary = player.call("exterior_resources")
	var original_draws: Dictionary = streamer.call("cell_instances")
	_check(original.size() == 10, "initial grid must contain nine cells and its persistent cell")
	_check(original_draws.size() == 9, "initial LAND render count")
	player.call("set_facing", PI / 2.0, 0.0)
	Input.action_press("Forward")
	# Walking crosses x=4096, then allow the two-cell budget to finish its column.
	for i in range(30):
		player.call("advance", STEP)
	Input.action_release("Forward")
	for i in range(4):
		player.call("advance", STEP)
	var first: Dictionary = player.call("exterior_resources")
	var first_draws: Dictionary = streamer.call("cell_instances")
	_check(player.call("gameplay_cell") == _cell(1, 0), "walking did not change logical focus")
	_check(first.size() == 13, "first crossing must add three cells and retain hysteresis column")
	for cell in original:
		_check(first.has(cell) and first[cell] == original[cell], "retained collision changed: " + cell)
	for cell in original_draws:
		_check(first_draws.has(cell) and first_draws[cell] == original_draws[cell], "retained draw changed: " + cell)
	Input.action_press("Forward")
	for i in range(2400):
		player.call("advance", STEP)
		var position: Vector3 = player.call("position")
		if position.x > 8192.0 + 64.0:
			break
	Input.action_release("Forward")
	for i in range(4):
		player.call("advance", STEP)
	var second: Dictionary = player.call("exterior_resources")
	var second_draws: Dictionary = streamer.call("cell_instances")
	_check(player.call("gameplay_cell") == _cell(2, 0), "second boundary was not crossed")
	_check(second.size() == 13, "second crossing must unload the stale column")
	for y in range(-1, 2):
		_check(not second.has(_cell(-1, y)), "stale collision column remains")
		_check(not second_draws.has(_cell(-1, y)), "stale render column remains")
		_check(second.has(_cell(3, y)), "new collision column missing")
		_check(second_draws.has(_cell(3, y)), "new render column missing")
	_check(second.has("0:000200"), "world persistent cell was unloaded")
	_check(player.call("is_on_ground"), "walking lost terrain collision")
	print("EXTERIOR TEST: initial=", original.size(), " first=", first.size(), " second=", second.size(),
		" position=", player.call("position"), " failures=", failures)
	player.free()
	streamer.free()
	quit(0 if failures == 0 else 1)
