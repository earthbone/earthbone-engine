extends "res://walkthrough.gd"

## HST-26 exercises the real startup without preinstalling streamer collision.
const UNITS := 69.99125
const STEP := 1.0 / 60.0
var failures := 0
var item_file: int
var item_object: int

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _process(_delta: float) -> bool:
	return false

func _check(ok: bool, message: String) -> bool:
	if not ok:
		printerr("HST-26 FAIL: ", message)
		failures += 1
		quit(1)
	return ok

func _tick(count := 1) -> void:
	for i in count:
		await physics_frame
		player.call("advance", STEP)

func _key(pressed: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = KEY_E
	event.pressed = pressed
	Input.parse_input_event(event)
	Input.flush_buffered_events()

func _pose() -> Dictionary:
	for row: Dictionary in streamer.call("movable_reference_poses"):
		if row.file == item_file and row.object == item_object:
			return row
	return {}

func _center(body: RID) -> Vector3:
	var transform: Transform3D = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM)
	var total := Vector3.ZERO
	var count := 0
	for i in PhysicsServer3D.body_get_shape_count(body):
		var data = PhysicsServer3D.shape_get_data(PhysicsServer3D.body_get_shape(body, i))
		var placement := transform * PhysicsServer3D.body_get_shape_transform(body, i)
		if data is PackedVector3Array:
			for point: Vector3 in data:
				total += placement * point
				count += 1
	var center := total / float(count) if count > 0 else transform.origin
	return Vector3(center.x, -center.z, center.y) * UNITS

func _start() -> void:
	create_timer(180.0).timeout.connect(func() -> void:
		_check(false, "headless walkthrough did not complete within 180 seconds"))
	await super._start()
	await hst_26_pickup_book_and_activator()

func hst_26_pickup_book_and_activator() -> void:
	if not loaded:
		return
	item_file = int(OS.get_environment("SOE_HST_26_ITEM_FILE"))
	item_object = int(OS.get_environment("SOE_HST_26_ITEM_OBJECT"))
	if not _check(not _pose().is_empty(), "walkthrough startup installs loose rigid bodies"):
		return
	if not _check(player.call("debug_watch_activation", item_file, item_object), "pickup Papyrus observer attached"):
		return
	var before: Dictionary = player.call("activation_reference_status", item_file, item_object)
	var aimed := false
	for offset: Vector3 in [Vector3(-85, 0, -10), Vector3(85, 0, -10), Vector3(0, -85, -10), Vector3(0, 85, -10)]:
		_check(player.call("queue_player_input", {"action": "move_to", "file": item_file, "object": item_object, "offset": offset, "match_rotation": false}), "approach queued")
		_check(player.call("step_player_input"), "approach applied")
		await _tick(30)
		var eye_godot := get_root().get_camera_3d().global_position
		var eye := Vector3(eye_godot.x, -eye_godot.z, eye_godot.y) * UNITS
		var direction := _center(_pose().body) - eye
		var yaw := atan2(direction.x, direction.y)
		var pitch := atan2(direction.z, Vector2(direction.x, direction.y).length())
		_check(player.call("queue_player_input", {"action": "turn", "rotation": Vector3(-pitch, 0, yaw)}), "aim queued")
		_check(player.call("step_player_input"), "aim applied")
		await _tick()
		var status: Dictionary = player.call("carry_status")
		if status.get("crosshair_file", -1) == item_file and status.get("crosshair_object", -1) == item_object:
			aimed = true
			break
	if not _check(aimed, "activation ray hits tankard"):
		return
	var prompt: Dictionary = player.call("activation_prompt")
	_check(not prompt.get("name", "").is_empty() and prompt.get("action", "") in ["Take", "Steal"], "crosshair name and pickup prompt")
	await process_frame
	await process_frame
	var label := player.find_child("ActivationPrompt", true, false) as Label
	_check(label != null and label.text.contains(prompt.name) and label.text.contains(prompt.action), "HUD displays prompt")
	_key(true)
	await _tick()
	_key(false)
	await _tick(2)
	var after: Dictionary = player.call("activation_reference_status", item_file, item_object)
	_check(after.count == before.count + 1 and not after.enabled, "mapped E increases inventory and disables reference")
	_check(after.activations == 1, "pickup OnActivate ran once")
	if prompt.stealing:
		_check(after.owned_count == before.owned_count + 1, "owned tankard remains stolen")
	for name: String in ["BOOK", "ACTI"]:
		var file := int(OS.get_environment("SOE_HST_26_" + name + "_FILE"))
		var object := int(OS.get_environment("SOE_HST_26_" + name + "_OBJECT"))
		_check(player.call("debug_watch_activation", file, object), name + " Papyrus observer attached")
		_check(player.call("activate_reference", file, object), name + " default activation")
		var status: Dictionary = player.call("activation_reference_status", file, object)
		_check(status.activations == 1, name + " OnActivate ran once")
		if name == "BOOK":
			_check(not status.enabled and player.call("is_menu_open"), "book picked up and opens menu")
			var text := player.find_child("BookText", true, false) as RichTextLabel
			_check(text != null and not text.text.is_empty(), "localized book text displayed")
			player.call("close_menu")
	print("HST-26 PASS" if failures == 0 else "HST-26 FAIL")
	quit(0 if failures == 0 else 1)
