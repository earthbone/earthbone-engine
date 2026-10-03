extends SceneTree

const TargetScript = preload("res://addons/soe_reference_gizmo/reference_target.gd")
const GizmoScript = preload("res://addons/soe_reference_gizmo/gizmo_plugin.gd")


class FakeDocument extends RefCounted:
	var current_position := Vector3(1.0, 2.0, 3.0)
	var current_rotation := Vector3(0.1, 0.2, 0.3)
	var edits := 0

	func reference_transform(_form_id: String) -> Dictionary:
		return {"position": current_position, "rotation": current_rotation}

	func set_reference_transform(_form_id: String, position: Vector3, rotation: Vector3) -> bool:
		current_position = position
		current_rotation = rotation
		edits += 1
		return true


func _initialize() -> void:
	var document := FakeDocument.new()
	var target := TargetScript.new()
	target.form_id = "1:000800"
	target.editor_document = document
	assert(target.load_record_transform())
	var restore := {"position": target.position, "rotation": target.rotation}
	var moved_position := Vector3(40.0, -5.0, 12.0)
	var moved_rotation := Vector3(0.4, 0.5, 0.6)
	var gizmo = GizmoScript.new()
	assert(gizmo.commit_target_transform(target, moved_position, moved_rotation, restore, false))
	assert(document.edits == 1)
	assert(document.current_position == moved_position)
	assert(target.position == moved_position)
	assert(gizmo.commit_target_transform(target, moved_position, moved_rotation, restore, true))
	assert(target.position == restore.position)
	assert(target.rotation == restore.rotation)
	print("edt_14_gizmo_test: PASS")
	quit(0)
