extends SceneTree

## Drops a body onto a real cell's collision inside Godot's own physics server,
## which is Jolt from 4.4 on. ENGINE-PLAN.md round 6, the `jolt` item. Run with:
##   SKYRIM_DATA="/path/to/Data" godot --headless --path godot --script physics_test.gd
##
## The point is the half that Rust tests cannot reach: that the Havok collision
## the host hands to PhysicsServer3D is a surface Godot's solver will rest a
## body on. The headless SimpleWorld is proved separately in
## crates/physics/tests/character.rs and crates/godot-host/tests/real_collision.rs.
##
## Exits 0 when a body falls and comes to rest on the cell, 1 otherwise.

var streamer: Node3D
var scene_root: Node3D
var built := false
var body: RigidBody3D
var frame := 0
var start_y := 0.0
var rest_frames := 0
var last_y := 0.0
var done := false

func _initialize() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	print("Godot version: ", Engine.get_version_info().string)
	print("physics engine: ", ProjectSettings.get_setting("physics/3d/physics_engine", "(default)"))
	if data == "":
		print("SKYRIM_DATA not set, skipping")
		quit(0)
		return
	if not ClassDB.class_exists("CellStreamerNode"):
		printerr("FAIL: CellStreamerNode is not registered, the extension did not load")
		quit(1)
		return

	var root := Node3D.new()
	root.name = "Root"
	get_root().add_child(root)

	streamer = ClassDB.instantiate("CellStreamerNode")
	streamer.name = "Streamer"
	root.add_child(streamer)

	if not streamer.call("load_data", data):
		printerr("FAIL: load_data(", data, ") returned false")
		quit(1)
		return
	scene_root = root


## The node has no World3D until the tree is actually running, so the space
## only exists from the first physics frame. Building collision in
## `_initialize` fails with "outside a 3D world".
func _build() -> void:
	var bodies: int = streamer.call("stream_cell_collision", "WhiterunBanneredMare")
	print("static bodies built: ", bodies)
	print("collision stats: ", streamer.call("collision_stats"))
	if bodies <= 0:
		printerr("FAIL: no static bodies were built")
		quit(1)
		return

	# A small sphere is the clearest probe: no orientation to argue about, and
	# it cannot wedge in a corner the way a box can.
	body = RigidBody3D.new()
	var shape := CollisionShape3D.new()
	var sphere := SphereShape3D.new()
	sphere.radius = 0.3
	shape.shape = sphere
	body.add_child(shape)
	# The cell sits around the origin in Godot metres. Start above it.
	body.position = Vector3(0.0, 12.0, 0.0)
	scene_root.add_child(body)
	start_y = body.position.y
	last_y = start_y
	print("dropped from y=", start_y)


func _physics_process(_delta: float) -> bool:
	if done:
		return true
	if not built:
		built = true
		_build()
		return false
	if body == null:
		return true
	frame += 1
	var y := body.position.y
	if frame % 30 == 0:
		print("frame ", frame, " y=", "%.3f" % y, " sleeping=", body.sleeping)
	# Resting means the height stopped changing, not that Godot marked it
	# asleep, since sleep thresholds vary between physics engines.
	if abs(y - last_y) < 0.001:
		rest_frames += 1
	else:
		rest_frames = 0
	last_y = y

	if rest_frames >= 30:
		done = true
		print("came to rest at y=", "%.3f" % y, " after ", frame, " frames")
		if y < start_y - 0.05 and y > -50.0:
			print("RESULT: PASS")
			quit(0)
		else:
			printerr("FAIL: rested at ", y, " from ", start_y, ", nothing caught it")
			quit(1)
		return true

	if frame > 900:
		done = true
		printerr("FAIL: still falling after ", frame, " frames, y=", y)
		quit(1)
		return true
	return false
