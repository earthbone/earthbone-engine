extends "res://hst_25_attack_test.gd"

## CMB-40: a released arrow is a rendered model that flies along its arc, hits
## the resident actor for damage, and stays stuck where it landed.
## Fails before the change: no flight node exists and health never drops.

func _check_weapons() -> void:
	await super._check_weapons()
	_check(contact_body != null and not contact_target.is_empty(), "arrow test needs the contact fixture")
	var origin: Vector3 = player.global_position + Vector3(0, 1.6, 0)
	var direction := Vector3(0, 0, -1)
	var distance := 0.0
	var space := player.get_world_3d().direct_space_state
	for step in 16:
		var candidate := Vector3(sin(step * TAU / 16.0), 0, -cos(step * TAU / 16.0))
		var query := PhysicsRayQueryParameters3D.create(origin, origin + candidate * 30.0)
		var hit := space.intersect_ray(query)
		var clear := 30.0 if hit.is_empty() else origin.distance_to(hit.position)
		if clear > distance:
			distance = clear
			direction = candidate
	distance = minf(distance - 1.0, 6.0)
	print("CMB-40 clear line direction=", direction, " target distance=", distance)
	_check(distance >= 3.0, "no clear line of at least 4 m around the player")
	contact_body.position = origin + direction * distance
	PhysicsServer3D.body_set_state(contact_body.get_rid(), PhysicsServer3D.BODY_STATE_TRANSFORM, Transform3D(Basis.IDENTITY, origin + direction * distance))
	await _steps(2)
	var health_before := _health()
	print("CMB-40 server body at ", PhysicsServer3D.body_get_state(contact_body.get_rid(), PhysicsServer3D.BODY_STATE_TRANSFORM).origin, " node at ", contact_body.global_position)
	var healths := {}
	for body: Dictionary in player.call("contact_actor_bodies"):
		healths[body.object] = [body.position, float(player.call("gameplay_actor_value", body.file, body.object, "health"))]
	print("CMB-40 bodies=", healths, " player=", player.global_position, " flat_origin=", origin)
	var arrows_before: int = player.call("play_path_item_count", 0, 0x1397d)
	var nodes_before: Array = player.call("ranged_flight_nodes")
	_check(player.call("fire_actor_arrow", 0, 0x14, origin, direction), "fire_actor_arrow was refused")
	_check(player.call("play_path_item_count", 0, 0x1397d) == arrows_before - 1, "release did not consume one arrow")
	var nodes: Array = []
	for candidate: Node3D in player.call("ranged_flight_nodes"):
		if not candidate in nodes_before:
			nodes.append(candidate)
	_check(nodes.size() == 1, "expected one new rendered arrow node after release: %d new, %d before" % [nodes.size(), nodes_before.size()])
	if nodes.size() != 1: return
	var node: Node3D = nodes[0]
	var meshes := 0
	for child in node.get_children():
		if child is MeshInstance3D: meshes += 1
	_check(meshes > 0, "arrow node has no mesh")
	var first := node.global_position
	player.call("advance", 1.0 / 60.0)
	_check(node.global_position.distance_to(first) > 0.05, "arrow node did not move along its flight")
	for _step in 30:
		player.call("advance", 1.0 / 60.0)
	_check(_health() < health_before, "arrow contact did not damage the actor: %f -> %f" % [health_before, _health()])
	for body: Dictionary in player.call("contact_actor_bodies"):
		var now := float(player.call("gameplay_actor_value", body.file, body.object, "health"))
		if not is_equal_approx(now, healths[body.object][1]):
			print("CMB-40 damaged ", body.object, " at ", body.position, " ", healths[body.object][1], " -> ", now)
	var stuck := node.global_position
	player.call("advance", 1.0 / 60.0)
	_check(is_instance_valid(node) and node.global_position.distance_to(stuck) < 0.01, "arrow did not stay at its impact point")
	_check(node.global_position.distance_to(contact_body.position) < 1.0, "arrow stopped away from the actor: impact %s body %s origin %s" % [node.global_position, contact_body.position, origin])
	print("CMB-40 health %f -> %f, mesh=%d, impact=%s" % [health_before, _health(), meshes, node.global_position])
