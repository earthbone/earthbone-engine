extends SceneTree

## PRF-25 physics report scene. It exercises active rigid bodies and a fixed
## 100-ray workload for each physics frame, then checks every reported field.

var failures := 0
var player: Node3D
const Overlay := preload("res://system_overlay.gd")


class RayWork extends Node:
	var tree_ref: SceneTree
	var profiled_player: Node3D
	var total := 0

	func _physics_process(_delta: float) -> void:
		profiled_player.call("advance", 1.0 / 60.0)
		var space := tree_ref.root.world_3d.direct_space_state
		for index in 100:
			var query := PhysicsRayQueryParameters3D.create(
				Vector3(float(index) * 0.01, 2.0, 0.0),
				Vector3(float(index) * 0.01, -2.0, 0.0))
			space.intersect_ray(query)
		total += 100


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("PRF-25 FAIL: ", message)


func _run() -> void:
	if not ClassDB.class_exists("PlayerNode"):
		printerr("PlayerNode extension is not loaded")
		quit(1)
		return
	player = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	var bodies := Node3D.new()
	root.add_child(bodies)
	for index in 3:
		var body := RigidBody3D.new()
		body.name = "body%d" % index
		body.position = Vector3(float(index) * 0.2, 1.0 + float(index), 0.0)
		var shape := CollisionShape3D.new()
		shape.shape = CapsuleShape3D.new()
		body.add_child(shape)
		bodies.add_child(body)
		if index > 0:
			var joint := PinJoint3D.new()
			joint.name = "joint%d" % index
			joint.position = body.position.lerp(
				Vector3(float(index - 1) * 0.2, float(index), 0.0), 0.5)
			joint.node_a = NodePath("../body%d" % (index - 1))
			joint.node_b = NodePath("../body%d" % index)
			bodies.add_child(joint)
	var rays := RayWork.new()
	rays.tree_ref = self
	rays.profiled_player = player
	root.add_child(rays)
	for _frame in 4:
		await physics_frame
		await process_frame
	var report: Dictionary = player.call("physics_frame_breakdown")
	var overlay := Overlay.new()
	overlay.player = player
	root.add_child(overlay)
	overlay.refresh()
	_check(overlay.label.text.contains("Physics detail:") and overlay.label.text.contains("raycast"),
		"F3 overlay did not show physics as one system with a query breakdown")
	_check(rays.total >= 400, "scene did not issue 100 rays on each physics frame")
	_check(report.steps >= 1, "physics step count was not reported")
	_check(report.catchup_steps >= 0, "catch-up count was negative")
	for field in ["jolt_step_ms", "active_bodies", "collision_pairs", "islands"]:
		_check(is_finite(float(report[field])) and float(report[field]) >= 0.0,
			"invalid Physics 3D monitor " + field)
	for kind in ["raycast", "shape_cast", "overlap"]:
		_check(report.queries.has(kind), "missing query category " + kind)
		for system in report.queries[kind]:
			var row: Dictionary = report.queries[kind][system]
			_check(row.count >= 0 and is_finite(row.ms) and row.ms >= 0.0,
				"invalid %s query values for %s" % [kind, system])
		_check(report.queries[kind].size() == 14, "query category lacks requesting systems")
	overlay.queue_free()
	rays_cleanup(rays, bodies)
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " prf_25 physics")
	quit(0 if failures == 0 else 1)


func rays_cleanup(rays: Node, bodies: Node3D) -> void:
	rays.queue_free()
	bodies.queue_free()
	player.queue_free()
