extends SceneTree

## Interactive movement prototype. Requires SKYRIM_DATA and the release extension.
## WASD move, mouse looks, Space jumps, Ctrl sneaks, F changes camera, Esc exits.

var player: Node3D
var frames := 0
var loaded := false

func _initialize() -> void:
	call_deferred("_start")

func _start() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("Set SKYRIM_DATA and build/import the Godot extension first")
		quit(1)
		return
	var scene := Node3D.new()
	get_root().add_child(scene)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	scene.add_child(streamer)
	if not streamer.call("load_data", data) or streamer.call("stream_cell", "WhiterunBanneredMare") <= 0:
		printerr("Could not render the Bannered Mare")
		quit(1)
		return
	var lighting := WorldEnvironment.new()
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.05, 0.05, 0.07)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color(0.7, 0.65, 0.6)
	environment.ambient_light_energy = 0.8
	lighting.environment = environment
	scene.add_child(lighting)
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-55, -35, 0)
	scene.add_child(sun)
	player = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", OS.get_environment("SOE_CAPTURE_PATH").is_empty())
	scene.add_child(player)
	if not player.call("load_data", data) or player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) <= 0:
		printerr("Could not enter the Bannered Mare")
		quit(1)
		return
	print("WASD move, mouse look, Space jump, Ctrl sneak, F camera, Esc exit")
	loaded = true

func _process(_delta: float) -> bool:
	if loaded:
		frames += 1
		var capture := OS.get_environment("SOE_CAPTURE_PATH")
		if frames == 120 and not capture.is_empty():
			var error := get_root().get_texture().get_image().save_png(capture)
			print("CAPTURE: ", capture, " error=", error)
			quit(0 if error == OK else 1)
	if Input.is_key_pressed(KEY_ESCAPE):
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
		quit(0)
	return false
