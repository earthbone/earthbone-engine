extends SceneTree

## Server-level Jolt survey, independent of game assets and the GDExtension.
## Matches the host's world-space triangle meshes on bodies at the origin.
## Source: engine-refs/godot-docs/classes/class_physicsserver3d.rst.
## Run: godot --headless --path godot --script large_world_physics_test.gd

const CELL_METRES := 4096.0 * 0.0142875
const CONTACT_ERROR := 0.03
const RADIUS := 0.3
const DIRECTIONS := [Vector3.RIGHT, Vector3.LEFT, Vector3.FORWARD, Vector3.BACK]

var space: RID
var shapes: Array[RID] = []
var bodies: Array[RID] = []
var probes: Array[RID] = []
var offsets: Array[Vector3] = [Vector3.ZERO]
var frame := 0
var finished := false


func _initialize() -> void:
	print("Godot: ", Engine.get_version_info().string)
	if ProjectSettings.get_setting("physics/3d/physics_engine") != "Jolt Physics":
		_fail("requires Jolt Physics")
		return
	for cell in [64, 65, 128, 256]:
		for direction in [Vector3(1, 0, 0), Vector3(-1, 0, 0),
			Vector3(0, 0, 1), Vector3(0, 0, -1), Vector3(1, 0, 1),
			Vector3(-1, 0, 1), Vector3(1, 0, -1), Vector3(-1, 0, -1)]:
			offsets.append(direction * cell * CELL_METRES)
	space = PhysicsServer3D.space_create()
	PhysicsServer3D.space_set_active(space, true)
	var sphere := PhysicsServer3D.sphere_shape_create()
	PhysicsServer3D.shape_set_data(sphere, RADIUS)
	shapes.append(sphere)
	for offset in offsets:
		var faces := PackedVector3Array()
		_quad(faces, offset + Vector3(-10, 0, -10), offset + Vector3(-10, 0, 10),
			offset + Vector3(10, 0, 10), offset + Vector3(10, 0, -10))
		for direction in DIRECTIONS:
			var centre: Vector3 = offset + direction * 3.0
			var side: Vector3 = direction.cross(Vector3.UP) * 2.0
			_quad(faces, centre - side, centre + side,
				centre + side + Vector3.UP * 3.0, centre - side + Vector3.UP * 3.0)
		var mesh := PhysicsServer3D.concave_polygon_shape_create()
		PhysicsServer3D.shape_set_data(mesh, {"faces": faces, "backface_collision": true})
		shapes.append(mesh)
		_body(mesh, PhysicsServer3D.BODY_MODE_STATIC, Vector3.ZERO)
		probes.append(_body(sphere, PhysicsServer3D.BODY_MODE_RIGID, offset + Vector3.UP * 4.0))


func _quad(faces: PackedVector3Array, a: Vector3, b: Vector3, c: Vector3, d: Vector3) -> void:
	faces.append_array(PackedVector3Array([a, b, c, a, c, d]))


func _body(shape: RID, mode: int, position: Vector3) -> RID:
	var body := PhysicsServer3D.body_create()
	PhysicsServer3D.body_set_mode(body, mode)
	PhysicsServer3D.body_add_shape(body, shape)
	PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM,
		Transform3D(Basis.IDENTITY, position))
	PhysicsServer3D.body_set_space(body, space)
	bodies.append(body)
	return body


func _physics_process(_delta: float) -> bool:
	if finished:
		return true
	frame += 1
	if frame != 240:
		return false
	var state := PhysicsServer3D.space_get_direct_state(space)
	var max_contact_error := 0.0
	var max_ray_error := 0.0
	var max_sweep_error := 0.0
	for i in offsets.size():
		var offset := offsets[i]
		var transform: Transform3D = PhysicsServer3D.body_get_state(probes[i], PhysicsServer3D.BODY_STATE_TRANSFORM)
		var error := transform.origin.distance_to(offset + Vector3.UP * RADIUS)
		var velocity: Vector3 = PhysicsServer3D.body_get_state(probes[i], PhysicsServer3D.BODY_STATE_LINEAR_VELOCITY)
		if not velocity.is_finite() or velocity.length() > 0.01:
			_fail("body still moving at %s: %s" % [offset, velocity])
			return true
		max_contact_error = max(max_contact_error, error)
		if not is_finite(error) or error > CONTACT_ERROR:
			_fail("floor contact at %s, error %s" % [offset, error])
			return true
		for direction in DIRECTIONS:
			var eye := offset + Vector3.UP
			var query := PhysicsRayQueryParameters3D.create(eye, eye + direction * 5.0)
			query.exclude = [probes[i]]
			var hit := state.intersect_ray(query)
			if hit.is_empty():
				_fail("ray missed at %s facing %s" % [offset, direction])
				return true
			error = hit.position.distance_to(eye + direction * 3.0)
			max_ray_error = max(max_ray_error, error)
			if not is_finite(error) or error > CONTACT_ERROR:
				_fail("ray error %s at %s facing %s" % [error, offset, direction])
				return true
			var motion := PhysicsTestMotionParameters3D.new()
			motion.from = Transform3D(Basis.IDENTITY, eye)
			motion.motion = direction * 5.0
			var result := PhysicsTestMotionResult3D.new()
			if not PhysicsServer3D.body_test_motion(probes[i], motion, result):
				_fail("sweep missed at %s facing %s" % [offset, direction])
				return true
			error = abs(result.get_travel().length() - (3.0 - RADIUS))
			max_sweep_error = max(max_sweep_error, error)
			if not is_finite(error) or error > CONTACT_ERROR:
				_fail("sweep error %s at %s facing %s" % [error, offset, direction])
				return true
	print("PASS: ", offsets.size(), " placements, 132 rays, 132 sweeps through +/-256 cells")
	print("maximum errors in metres: contact=", max_contact_error,
		" ray=", max_ray_error, " sweep=", max_sweep_error)
	_finish(0)
	return true


func _fail(message: String) -> void:
	printerr("FAIL: ", message)
	_finish(1)


func _finish(code: int) -> void:
	finished = true
	for body in bodies:
		PhysicsServer3D.free_rid(body)
	for shape in shapes:
		PhysicsServer3D.free_rid(shape)
	if space.is_valid():
		PhysicsServer3D.free_rid(space)
	quit(code)
