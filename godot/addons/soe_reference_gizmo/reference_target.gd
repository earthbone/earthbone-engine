@tool
extends Node3D

var form_id := ""
var editor_document: Object
var record_position := Vector3.ZERO
var record_rotation := Vector3.ZERO


func load_record_transform() -> bool:
	if not is_instance_valid(editor_document):
		return false
	var record: Dictionary = editor_document.reference_transform(form_id)
	if not record.has("position") or not record.has("rotation"):
		return false
	record_position = record.position
	record_rotation = record.rotation
	global_position = record.position
	global_rotation = record.rotation
	return true


func commit_gizmo_transform(next_position: Vector3, next_rotation: Vector3, restore: Variant, cancel: bool) -> bool:
	if cancel:
		record_position = restore.position
		record_rotation = restore.rotation
		global_position = record_position
		global_rotation = record_rotation
		update_gizmos()
		return true
	if not is_instance_valid(editor_document):
		record_position = restore.position
		record_rotation = restore.rotation
		global_position = record_position
		global_rotation = record_rotation
		update_gizmos()
		return false
	if not editor_document.set_reference_transform(form_id, next_position, next_rotation):
		record_position = restore.position
		record_rotation = restore.rotation
		global_position = record_position
		global_rotation = record_rotation
		update_gizmos()
		return false
	global_position = next_position
	global_rotation = next_rotation
	record_position = next_position
	record_rotation = next_rotation
	update_gizmos()
	return true
