extends "res://hst_25_attack_test.gd"

## CMB-40: a released arrow is a rendered model that flies along its arc, hits
## the resident actor for damage, and stays stuck where it landed.
## Fails before the change: no flight node exists and health never drops.

func _check_weapons() -> void:
	await super._check_weapons()
	_check(contact_body != null and not contact_target.is_empty(), "arrow test needs the contact fixture")
	var origin: Vector3 = player.global_position + Vector3(0, 1.6, 0)
	var direction := Vector3(0, 0, -1)
	contact_body.position = origin + direction * 6.0
	PhysicsServer3D.body_set_state(contact_body.get_rid(), PhysicsServer3D.BODY_STATE_TRANSFORM, Transform3D(Basis.IDENTITY, origin + direction * 6.0))
	await _steps(2)
	var health_before := _health()
	var arrows_before: int = player.call("play_path_item_count", 0, 0x1397d)
	_check(player.call("fire_actor_arrow", 0, 0x14, origin, direction), "fire_actor_arrow was refused")
	_check(player.call("play_path_item_count", 0, 0x1397d) == arrows_before - 1, "release did not consume one arrow")
	var nodes: Array = player.call("ranged_flight_nodes")
	_check(nodes.size() == 1, "no rendered arrow node after release: %d" % nodes.size())
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
	var stuck := node.global_position
	player.call("advance", 1.0 / 60.0)
	_check(is_instance_valid(node) and node.global_position.distance_to(stuck) < 0.01, "arrow did not stay at its impact point")
	_check(node.global_position.distance_to(contact_body.position) < 1.0, "arrow stopped away from the actor")
	print("CMB-40 health %f -> %f, mesh=%d, impact=%s" % [health_before, _health(), meshes, node.global_position])
