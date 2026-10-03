extends SceneTree

## EDT-15: the editor bridge hits host collision and converts its surface point.
var failures := 0
var body: RID
var shape: RID
var stage: Node3D


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func _run() -> void:
	if not ClassDB.class_exists("SoeFormTable"):
		printerr("FAIL: host extension did not register SoeFormTable")
		quit(1)
		return
	stage = Node3D.new()
	root.add_child(stage)
	var space: RID = stage.get_world_3d().space
	shape = PhysicsServer3D.box_shape_create()
	PhysicsServer3D.shape_set_data(shape, Vector3(1.0, 0.1, 1.0))
	body = PhysicsServer3D.body_create()
	PhysicsServer3D.body_set_mode(body, PhysicsServer3D.BODY_MODE_STATIC)
	PhysicsServer3D.body_add_shape(body, shape)
	PhysicsServer3D.body_set_space(body, space)
	await physics_frame

	var table := SoeFormTable.new()
	var hit: Dictionary = table.query_ground_collision(space, 0.0, 0.0, 100.0, 0.0, 200.0)
	_check(hit.get("hit", false), "host ray missed the test collision surface")
	var position: Vector3 = hit.get("position", Vector3.INF)
	_check(position.is_finite(), "host ray returned no finite Skyrim-space position")
	_check(absf(position.z - 7.0) < 0.1,
		"host collision height was not converted to Skyrim Z: %.3f" % position.z)

	PhysicsServer3D.free_rid(body)
	PhysicsServer3D.free_rid(shape)
	stage.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " EDT-15 host collision query")
	quit(0 if failures == 0 else 1)
