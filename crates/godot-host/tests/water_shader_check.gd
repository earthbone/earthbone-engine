# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree

func _initialize() -> void:
	var shader := Shader.new()
	shader.code = FileAccess.get_file_as_string("res://water.gdshader")
	if not shader.code.contains("depth_texture : hint_depth_texture"):
		printerr("water shader missing depth sampler")
		quit(1)
		return
	var uniforms := shader.get_shader_uniform_list()
	var names: Array[String] = []
	for item in uniforms:
		names.append(item.name)
	for name in ["noise_0", "noise_1", "noise_2", "flow_normals", "has_noise", "has_flow", "mesh_uv", "shallow_color", "deep_color", "reflection_color", "sun_power"]:
		if not names.has(name):
			printerr("water shader missing uniform: ", name)
			quit(1)
			return
	var material := ShaderMaterial.new()
	material.shader = shader
	# A missing texture keeps the WATR values and disabled normal samplers.
	material.set_shader_parameter("shallow_color", Color(0.1, 0.2, 0.3))
	material.set_shader_parameter("base_alpha", 0.5)
	material.set_shader_parameter("has_noise", 0)
	if material.get_shader_parameter("shallow_color") != Color(0.1, 0.2, 0.3) or material.get_shader_parameter("base_alpha") != 0.5:
		quit(1)
		return
	print("water shader: PASS ", uniforms.size(), " uniforms on ", Engine.get_version_info().string)
	quit(0)
