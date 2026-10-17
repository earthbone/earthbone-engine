# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree

## Fixed before the first render: quadrant colors agree within 0.04 per channel.
func _initialize() -> void:
    call_deferred("_run")

func _run() -> void:
    var failures := 0
    var viewport := SubViewport.new()
    viewport.size = Vector2i(256, 256)
    viewport.own_world_3d = true
    viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
    root.add_child(viewport)
    var camera := Camera3D.new()
    viewport.add_child(camera)
    camera.position.z = 2.0
    camera.current = true
    var geometry := MeshInstance3D.new()
    geometry.mesh = QuadMesh.new()
    geometry.mesh.size = Vector2(2, 2)
    viewport.add_child(geometry)
    var base := StandardMaterial3D.new()
    base.render_priority = -1
    geometry.material_override = base
    var code := FileAccess.get_file_as_string(OS.get_environment("SOE_TERRAIN_SHADER_PATH"))
    code = code.replace("shader_type spatial;", "shader_type spatial; render_mode unshaded;")
    var uniforms := ""
    for layer in 6:
        uniforms += "uniform sampler2D layer_%d : source_color, filter_nearest, repeat_enable;\n" % layer
    for mask in 2:
        uniforms += "uniform sampler2D weights_%d : filter_linear, repeat_disable;\n" % mask
    var blend := ""
    for layer in range(1, 6):
        blend += "color = mix(color, texture(layer_%d, uv * 4.0).rgb, texture(weights_%d, weights_uv).%s);\n" % [layer, (layer-1)/4, ["r", "g", "b", "a"][(layer-1)%4]]
    code = code.replace("LAYER_UNIFORMS", uniforms)
    code = code.replace("LAYER_BLEND", blend).replace("WEIGHT_SIDE", "17.0")
    var shader := Shader.new()
    shader.code = code
    var colors := [Color.RED, Color.GREEN, Color.BLUE, Color.YELLOW]
    var tail: Material = base
    for q in 4:
        var material := ShaderMaterial.new()
        material.shader = shader
        material.set_shader_parameter("quadrant", Vector2(q % 2, q / 2))
        for layer in 6:
            var tile := Image.create(1, 1, false, Image.FORMAT_RGBA8)
            tile.fill(colors[q] if layer == 5 else Color.WHITE)
            material.set_shader_parameter("layer_%d" % layer, ImageTexture.create_from_image(tile))
        for mask in 2:
            var weights := Image.create(17, 17, false, Image.FORMAT_RGBA8)
            weights.fill(Color(1, 0, 0, 0) if mask == 1 else Color(0, 0, 0, 0))
            material.set_shader_parameter("weights_%d" % mask, ImageTexture.create_from_image(weights))
        tail.next_pass = material
        tail = material
    for frame in 6:
        await process_frame
    await RenderingServer.frame_post_draw
    var image := viewport.get_texture().get_image()
    var expected := [Color.BLUE, Color.YELLOW, Color.RED, Color.GREEN]
    var samples := [Vector2i(100, 100), Vector2i(156, 100), Vector2i(100, 156), Vector2i(156, 156)]
    for i in 4:
        var pixel := image.get_pixelv(samples[i])
        var error := maxf(absf(pixel.r-expected[i].r), maxf(absf(pixel.g-expected[i].g), absf(pixel.b-expected[i].b)))
        print("R4 TERRAIN MATERIAL sample=", samples[i], " actual=", pixel, " expected=", expected[i], " error=", error)
        failures += int(error > 0.04)
    viewport.free()
    await process_frame
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 terrain material")
    quit(0 if failures == 0 else 1)
