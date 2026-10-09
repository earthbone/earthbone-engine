extends SceneTree

const RecordSky := preload("res://record_sky.gd")
const SPEED_TOLERANCE := 0.03
var failed := false

func _initialize() -> void:
	call_deferred("_run")

func check(value: bool, message: String) -> void:
	if not value:
		failed = true
		printerr("RND-25 FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("RND-25 SKIP: SKYRIM_DATA unset")
		quit()
		return
	var scene := Node3D.new()
	root.add_child(scene)
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	scene.add_child(streamer)
	if not streamer.call("load_data", data):
		check(false, "load installed data")
		quit(1)
		return
	var sky := RecordSky.new()
	scene.add_child(sky)
	sky.set_process(false)
	check(sky.use_streamer(streamer, "WhiterunOrigin", 12.0), "initialize record sky")
	check(sky.records.set_weather("SkyrimCloudy", true), "select Tamriel SkyrimCloudy")
	check(sky.refresh(), "read SkyrimCloudy weather")
	var clouds: Array = sky.last_frame.clouds
	var speeds: PackedVector2Array = sky.material.get_shader_parameter("cloud_speeds")
	var moving_layers := 0
	for index in clouds.size():
		var authored: Vector2 = clouds[index].speed
		var expected := authored / 400.0
		if authored.length() > 0.000001:
			moving_layers += 1
		for axis in 2:
			var source_rate := authored[axis]
			var actual_rate := speeds[index][axis]
			var derived_rate := source_rate / 400.0
			var allowed_error := maxf(absf(derived_rate) * SPEED_TOLERANCE, 0.0000001)
			check(absf(actual_rate - derived_rate) <= allowed_error,
				"layer %s axis %s UV rate %s matches WTHR-derived %s (expected %s)" % [index, axis, actual_rate, derived_rate, expected])
	check(moving_layers > 0, "SkyrimCloudy contains at least one moving cloud layer")
	print("RND-25 weather=", sky.last_frame.weather, " layers=", clouds.size(), " moving=", moving_layers, " scale=1/400")
	print("RND-25 RESULT: ", "FAIL" if failed else "PASS")
	scene.queue_free()
	await process_frame
	quit(1 if failed else 0)
