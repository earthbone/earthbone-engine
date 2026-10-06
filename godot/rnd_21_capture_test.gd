extends SceneTree

## Installed-data capture check for RND-21. Brightness bounds are fixed before
## the first capture: average linear luminance 0.02 to 0.65, fewer than one
## percent fully clipped pixels.
const RecordSky := preload("res://record_sky.gd")
const Warmup := preload("res://render_warmup.gd")
const MIN_AVERAGE := 0.02
const MAX_AVERAGE := 0.65
const MAX_CLIPPED_RATIO := 0.01
const CAPTURE := "res://../target/rnd_21_bannered_mare.png"

var player: Node3D
var frames := 0
var loaded := false


func _initialize() -> void:
	call_deferred("_start")


func _start() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("RND-21 requires SKYRIM_DATA and the built host extension")
		quit(1)
		return
	var scene := Node3D.new()
	get_root().add_child(scene)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	scene.add_child(streamer)
	if not streamer.call("begin_load_data", data):
		printerr("RND-21 could not start loading installed data")
		quit(1)
		return
	var state: int = streamer.call("poll_load_data")
	while state == 0:
		await process_frame
		state = streamer.call("poll_load_data")
	if state < 0 or streamer.call("stream_cell", "WhiterunBanneredMare") <= 0:
		printerr("RND-21 could not stream the Bannered Mare")
		quit(1)
		return
	var lighting: Dictionary = streamer.call("cell_lighting", "WhiterunBanneredMare")
	if lighting.is_empty():
		printerr("RND-21 Bannered Mare has no resolved cell lighting")
		quit(1)
		return
	var sky := RecordSky.new()
	scene.add_child(sky)
	sky.set_process(false)
	if not sky.use_streamer(streamer, "WhiterunBanneredMare") or sky.sun.visible:
		printerr("RND-21 interior controller failed or added a weather sun")
		quit(1)
		return
	player = ClassDB.instantiate("PlayerNode")
	player.set("auto_step", false)
	player.set("capture_mouse", false)
	scene.add_child(player)
	if not player.call("use_streamer_data", streamer) or player.call("enter_cell", "WhiterunBanneredMare", -125.0, -800.0, 120.0) <= 0:
		printerr("RND-21 could not enter the Bannered Mare")
		quit(1)
		return
	var rendering: Dictionary = await Warmup.prepare()
	if not rendering.error.is_empty():
		printerr("RND-21 rendering preparation failed: " + rendering.error)
		quit(1)
		return
	await process_frame
	await process_frame
	loaded = true


func _process(_delta: float) -> bool:
	if not loaded:
		return false
	frames += 1
	player.set("auto_step", false)
	if frames < 8:
		return false
	var image := get_root().get_texture().get_image()
	var capture := ProjectSettings.globalize_path(CAPTURE)
	if image.save_png(capture) != OK:
		printerr("RND-21 could not save the capture")
		quit(1)
		return false
	var luminance_sum := 0.0
	var clipped := 0
	var total := image.get_width() * image.get_height()
	for y in image.get_height():
		for x in image.get_width():
			var pixel := image.get_pixel(x, y)
			luminance_sum += 0.2126 * pow(pixel.r, 2.2) + 0.7152 * pow(pixel.g, 2.2) + 0.0722 * pow(pixel.b, 2.2)
			if pixel.r >= 0.98 and pixel.g >= 0.98 and pixel.b >= 0.98:
				clipped += 1
	var average := luminance_sum / float(total)
	var clipped_ratio := float(clipped) / float(total)
	print("RND-21 average_linear_luminance=%.4f clipped_pixel_ratio=%.5f" % [average, clipped_ratio])
	var ok := average >= MIN_AVERAGE and average <= MAX_AVERAGE and clipped_ratio < MAX_CLIPPED_RATIO
	quit(0 if ok else 1)
	return false
