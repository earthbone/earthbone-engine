extends SceneTree

## Streams an exterior block and saves a fixed set of camera shots, so a
## rendering fault is checked from every side instead of one framing.
##
## Shots: eight on a high orbit, eight on a low orbit, eight looking outward
## from the block centre, and one from straight above. Each waits
## SETTLE_FRAMES before saving, so a late texture upload shows up.
##
## Environment variables:
##   SKYRIM_DATA         installed game Data folder (required)
##   SOE_CAPTURE_DIR     where the PNGs go (required)
##   SOE_BENCH_CELL      exterior anchor, default WhiterunOrigin
##   SOE_BENCH_RADIUS    block radius, default 1

const SETTLE_FRAMES := 10

var streamer: Node3D
var holder: Node3D
var camera: Camera3D
var out_dir := ""
var shots: Array = []
var shot := 0
var wait := 0
var ready_to_shoot := false


func _initialize() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	out_dir = OS.get_environment("SOE_CAPTURE_DIR")
	if data.is_empty() or out_dir.is_empty():
		printerr("RESULT: ERROR: SKYRIM_DATA and SOE_CAPTURE_DIR are required")
		quit(2)
		return
	var cell := OS.get_environment("SOE_BENCH_CELL")
	if cell.is_empty():
		cell = "WhiterunOrigin"
	var radius_text := OS.get_environment("SOE_BENCH_RADIUS")
	var radius := 1 if radius_text.is_empty() else int(radius_text)
	DirAccess.make_dir_recursive_absolute(out_dir)
	call_deferred("_setup", data, cell, radius)


func _setup(data: String, cell: String, radius: int) -> void:
	holder = Node3D.new()
	get_root().add_child(holder)
	streamer = ClassDB.instantiate("CellStreamerNode") as Node3D
	if streamer == null:
		printerr("RESULT: FAIL: could not instantiate CellStreamerNode")
		quit(1)
		return
	holder.add_child(streamer)
	if not streamer.call("load_data", data):
		printerr("RESULT: FAIL: load_data could not open %s" % data)
		quit(1)
		return
	var built: int = streamer.call("stream_exterior", cell, radius)
	if built <= 0:
		printerr("RESULT: FAIL: nothing built for %s radius %d" % [cell, radius])
		quit(1)
		return
	print("capture_instances %d" % built)
	_setup_view()
	ready_to_shoot = true


## Lighting matches bench.gd so the captures and the benchmark show the
## same scene.
func _setup_view() -> void:
	var bounds: AABB = streamer.call("scene_bounds")
	var centre := bounds.get_center()
	var reach: float = maxf(bounds.size.length() * 0.5, 1.0)
	camera = Camera3D.new()
	camera.far = reach * 8.0 + 100.0
	holder.add_child(camera)
	camera.make_current()
	var light := DirectionalLight3D.new()
	light.rotation_degrees = Vector3(-50.0, 30.0, 0.0)
	holder.add_child(light)
	var env := WorldEnvironment.new()
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.55, 0.65, 0.8)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color(0.6, 0.6, 0.65)
	environment.ambient_light_energy = 0.8
	env.environment = environment
	holder.add_child(env)
	# The streamer reports no ground height, so the outward ring sits near
	# the top of the bounds, above the terrain, and looks slightly down.
	var eye := Vector3(centre.x, bounds.position.y + bounds.size.y * 0.8, centre.z)
	for i in 8:
		var angle := TAU * i / 8.0
		var out := Vector3(sin(angle), 0.0, cos(angle))
		shots.append(["high_%03d" % (i * 45), centre + out * reach * 1.4 + Vector3.UP * reach * 0.6, centre])
		shots.append(["low_%03d" % (i * 45), centre + out * reach * 1.1 + Vector3.UP * reach * 0.12, centre])
		shots.append(["out_%03d" % (i * 45), eye, eye + out * reach - Vector3.UP * reach * 0.2])
	shots.append(["top", centre + Vector3.UP * reach * 1.8 + Vector3(0.0, 0.0, 0.01), centre])
	print("capture_extent %.1f m, %d shots" % [bounds.size.length(), shots.size()])


func _process(_delta: float) -> bool:
	if not ready_to_shoot:
		return false
	if wait == 0:
		if shot >= shots.size():
			print("RESULT: PASS: %d captures in %s" % [shots.size(), out_dir])
			quit(0)
			return true
		var s: Array = shots[shot]
		camera.position = s[1]
		camera.look_at(s[2], Vector3.UP)
		wait = SETTLE_FRAMES
		return false
	wait -= 1
	if wait == 0:
		var s: Array = shots[shot]
		var path := "%s/%02d_%s.png" % [out_dir, shot, s[0]]
		var err := get_root().get_texture().get_image().save_png(path)
		if err != OK:
			printerr("RESULT: FAIL: could not save %s: %s" % [path, error_string(err)])
			quit(1)
			return true
		shot += 1
	return false
