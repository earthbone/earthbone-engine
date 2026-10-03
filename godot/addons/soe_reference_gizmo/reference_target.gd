@tool
extends Node3D

var form_id := ""
var editor_document: Object


func load_record_transform() -> bool:
	if not is_instance_valid(editor_document):
		return false
	var record: Dictionary = editor_document.reference_transform(form_id)
	if not record.has("position") or not record.has("rotation"):
		return false
	position = record.position
	rotation = record.rotation
	return true


func commit_gizmo_transform(next_position: Vector3, next_rotation: Vector3, restore: Variant, cancel: bool) -> bool:
	if cancel:
		position = restore.position
		rotation = restore.rotation
		return true
	if not is_instance_valid(editor_document):
		return false
	if not editor_document.set_reference_transform(form_id, next_position, next_rotation):
		position = restore.position
		rotation = restore.rotation
		return false
	position = next_position
	rotation = next_rotation
	return true
