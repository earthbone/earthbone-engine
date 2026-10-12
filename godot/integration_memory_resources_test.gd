extends SceneTree

## Cache ownership follows Godot RefCounted resources and mesh material bindings.


func _initialize() -> void:
	call_deferred("run")


func model_meshes(streamer: Node, model: String = "quad.nif") -> Array:
	return streamer.call("model_meshes", model)


func resources_alive(material_ref: WeakRef, texture_ref: WeakRef) -> bool:
	return material_ref.get_ref() != null and texture_ref.get_ref() != null


func shaders_shared(streamer: Node) -> bool:
	var first_meshes: Array = model_meshes(streamer, "first.nif")
	var second_meshes: Array = model_meshes(streamer, "second.nif")
	if first_meshes.size() != 1 or second_meshes.size() != 1:
		return false
	var first: ShaderMaterial = first_meshes[0].surface_get_material(0)
	var second: ShaderMaterial = second_meshes[0].surface_get_material(0)
	if first == second or first.get_shader() != second.get_shader():
		return false
	first.set_shader_parameter("has_normal", true)
	return second.get_shader_parameter("has_normal") != true and first.get_shader_parameter("albedo_texture") != second.get_shader_parameter("albedo_texture")


func run() -> void:
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	if not streamer.call("load_data", OS.get_environment("SOE_MEMORY_FIXTURE")):
		printerr("MEMORY FAIL: synthetic source")
		quit(1)
		return
	var meshes: Array = model_meshes(streamer)
	if meshes.size() != 1:
		printerr("MEMORY FAIL: synthetic mesh")
		quit(1)
		return
	var material: StandardMaterial3D = meshes[0].surface_get_material(0)
	var texture: Texture2D = material.get_texture(BaseMaterial3D.TEXTURE_ALBEDO)
	if texture == null:
		printerr("MEMORY FAIL: fallback texture")
		quit(1)
		return
	var material_ref: WeakRef = weakref(material)
	var texture_ref: WeakRef = weakref(texture)
	material = null
	texture = null
	streamer.call("clear")
	if not resources_alive(material_ref, texture_ref):
		printerr("MEMORY FAIL: live mesh lost its resources")
		quit(1)
		return
	meshes.clear()
	streamer.call("clear")
	if material_ref.get_ref() != null or texture_ref.get_ref() != null:
		printerr("MEMORY FAIL: unused cache retains resources")
		quit(1)
		return
	if not shaders_shared(streamer):
		printerr("MEMORY FAIL: shader sharing changed material state")
		quit(1)
		return
	streamer.call("clear")
	streamer.free()
	print("integration_memory_resources: PASS")
	quit(0)
