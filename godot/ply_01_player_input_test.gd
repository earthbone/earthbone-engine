extends SceneTree

## PLY-01 compares actual fixed-step host events with the headless Game fixture.
var failures := 0

func _initialize() -> void:
	call_deferred("ply_01_windowed_matches_headless")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL ply_01: ", message)

func ply_01_windowed_matches_headless() -> void:
	var data := OS.get_environment("SOE_PLY_01_FIXTURE")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL ply_01: requires exported headless fixture and extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "fixture load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared records")
	player.call("enter_cell", "PlyStart", 0.0, 0.0, 0.0)
	_check(player.call("start_gameplay"), "Game start")
	var sequence: Array[Dictionary] = [
		{"action": "move_to", "file": 0, "object": 0x810, "offset": Vector3(0, 5, 0), "match_rotation": true},
		{"action": "turn", "rotation": Vector3(0, 0, 1)},
		{"action": "activate", "file": 0, "object": 0x810},
		{"action": "activate", "file": 0, "object": 0x811},
		{"action": "move_to", "file": 0, "object": 0x820, "offset": Vector3.ZERO, "match_rotation": false},
		{"action": "activate", "file": 0, "object": 0x820},
		{"action": "move_to", "file": 0, "object": 0x830, "offset": Vector3(0, 5, 0), "match_rotation": true},
		{"action": "activate", "file": 0, "object": 0x830},
		{"action": "choose_topic", "file": 0, "object": 0x841},
		{"action": "confirm_menu", "menu": "dialogue"},
		{"action": "wait"},
	]
	for command: Dictionary in sequence:
		_check(player.call("queue_player_input", command), "queue " + command.action)
	_check(player.call("player_input_pending") == sequence.size(), "one queue entry per command")
	for index in range(sequence.size()):
		_check(player.call("step_player_input"), "step " + str(index))
		if index == 1:
			var position: Vector3 = player.call("position")
			var facing: Vector2 = player.call("facing")
			_check(position.is_equal_approx(Vector3(10, 5, 0)), "controller move")
			_check(is_equal_approx(facing.x, 1.0), "controller yaw")
		if index == 3:
			_check(player.call("gameplay_cell") == "0:000801", "host load-door replacement")
		if index == 8:
			_check(player.call("is_menu_open"), "dialogue menu opened")
			_check(_has_response(player), "dialogue response presented")
		await process_frame
	_check(player.call("player_input_pending") == 0, "queue drained")
	var before: PackedStringArray = player.call("player_input_log")
	_check(not player.call("step_player_input"), "empty queue does not step")
	_check(before == player.call("player_input_log"), "empty queue preserves log")
	var expected := FileAccess.get_file_as_string(data.path_join("expected.log")).strip_edges().split("\n")
	_check(before == expected, "headless and windowed event logs differ\nexpected=" + str(expected) + "\nactual=" + str(before))
	if failures == 0:
		var image := root.get_texture().get_image()
		_check(image != null and not image.is_empty(), "window viewport")
		if image != null:
			_check(image.save_png(data.path_join("window.png")) == OK, "window capture")
	print("ply_01_windowed_matches_headless: ", "PASS" if failures == 0 else "FAIL", " events=", before.size())
	player.free()
	streamer.free()
	quit(0 if failures == 0 else 1)

func _has_response(node: Node) -> bool:
	if node is Label and node.text == "Hello from the shared runtime":
		return node.is_visible_in_tree()
	for child: Node in node.get_children():
		if _has_response(child):
			return true
	return false
