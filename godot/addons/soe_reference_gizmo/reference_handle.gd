@tool
extends RefCounted

const ReferenceTarget = preload("res://addons/soe_reference_gizmo/reference_target.gd")


static func drag_target_to_screen(target: Node3D, camera: Camera3D, screen_pos: Vector2) -> bool:
	var origin := camera.project_ray_origin(screen_pos)
	var direction := camera.project_ray_normal(screen_pos)
	var plane := Plane(-camera.global_basis.z, target.global_position)
	var hit: Variant = plane.intersects_ray(origin, direction)
	if not hit is Vector3:
		return false
	target.global_position = hit
	target.update_gizmos()
	return true


static func commit_target_transform(target: Node3D, next_position: Vector3, next_rotation: Vector3, restore: Variant, cancel: bool) -> bool:
	var committed := (target as ReferenceTarget).commit_gizmo_transform(next_position, next_rotation, restore, cancel)
	if committed:
		target.update_gizmos()
	return committed
