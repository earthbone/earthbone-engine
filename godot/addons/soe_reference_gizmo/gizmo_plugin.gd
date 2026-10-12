@tool
extends EditorNode3DGizmoPlugin

const ReferenceTarget = preload("res://addons/soe_reference_gizmo/reference_target.gd")
const ReferenceHandle = preload("res://addons/soe_reference_gizmo/reference_handle.gd")


func _init() -> void:
	create_material("axis", Color(0.95, 0.55, 0.12), false, true)
	create_handle_material("handles", true)


func _has_gizmo(node: Node3D) -> bool:
	return node is ReferenceTarget


func _get_gizmo_name() -> String:
	return "Skyrim Reference Transform"


func _redraw(gizmo: EditorNode3DGizmo) -> void:
	gizmo.clear()
	var lines := PackedVector3Array([Vector3.ZERO, Vector3.RIGHT])
	gizmo.add_lines(lines, get_material("axis", gizmo), false)
	gizmo.add_handles(PackedVector3Array([Vector3.RIGHT]), get_material("handles", gizmo), PackedInt32Array())


func _get_handle_name(_gizmo: EditorNode3DGizmo, _handle_id: int, _secondary: bool) -> String:
	return "Move reference"


func _get_handle_value(gizmo: EditorNode3DGizmo, _handle_id: int, _secondary: bool) -> Variant:
	var target := gizmo.get_node_3d() as ReferenceTarget
	return {"position": target.record_position, "rotation": target.record_rotation}


func _set_handle(gizmo: EditorNode3DGizmo, _handle_id: int, _secondary: bool, camera: Camera3D, screen_pos: Vector2) -> void:
	var target := gizmo.get_node_3d() as ReferenceTarget
	ReferenceHandle.drag_target_to_screen(target, camera, screen_pos)


func _commit_handle(gizmo: EditorNode3DGizmo, _handle_id: int, _secondary: bool, restore: Variant, cancel: bool) -> void:
	var target := gizmo.get_node_3d() as ReferenceTarget
	ReferenceHandle.commit_target_transform(target, target.global_position, target.record_rotation, restore, cancel)
