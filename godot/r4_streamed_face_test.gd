# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Fixed before baseline: each streamed face needs 500 warm skin pixels and
## mean red greater than blue by 0.03 under the walkthrough lighting.
var failures := 0

func _initialize() -> void:
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse", false)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 face: ", message)

func _run() -> void:
    _check(OS.get_environment("DISPLAY") == ":99", "private display")
    await super._start()
    if not loaded:
        quit(1)
        return
    var started := Time.get_ticks_msec()
    while Time.get_ticks_msec() - started < 60000:
        var counts: Dictionary = player.call("npc_visual_counts")
        if counts.drawn > 0 and counts.pending == 0:
            break
        await process_frame
    player.set("auto_step", false)
    for overlay in root.find_children("*", "CanvasLayer", true, false):
        overlay.hide()
    var actors: Array = player.call("npc_visuals")
    print("R4 residents=", actors)
    for actor: Dictionary in actors:
        if actor.node_id != 0:
            var resident := instance_from_id(actor.node_id)
            resident.set("playing", false)
            resident.set_process(false)
    var original_camera := root.get_camera_3d()
    var camera := Camera3D.new()
    camera.environment = original_camera.environment
    camera.attributes = original_camera.attributes
    player.get_parent().add_child(camera)
    camera.near = 0.01
    camera.fov = 30.0
    camera.current = true
    var checked := 0
    for actor: Dictionary in actors:
        if actor.node_id == 0 or actor.editor_id not in ["HuldaREF", "MikaelREF", "SinmirRef", "LuciaRef"]:
            continue
        var node := instance_from_id(actor.node_id)
        node.set("playing", false)
        node.set_process(false)
        var observation: Dictionary = node.call("play_path_observation")
        var head_rids: Array[RID] = []
        var offset := 0
        var bounds := AABB()
        var first := true
        var placement: Transform3D = observation.placement
        for shape: Dictionary in observation.shapes:
            var arrays: Array = shape.mesh.surface_get_arrays(0)
            var bones: PackedInt32Array = arrays[Mesh.ARRAY_BONES]
            var weights: PackedFloat32Array = arrays[Mesh.ARRAY_WEIGHTS]
            var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
            var count := 0
            for bone in bones:
                count = maxi(count, bone + 1)
            if shape.head_part_type == 1:
                head_rids.append(shape.rid)
                var material: Material = shape.mesh.surface_get_material(0)
                _check(material is ShaderMaterial, actor.editor_id + " has FaceGen shader")
                if material is ShaderMaterial:
                    _check(material.get_shader_parameter("specular_strength") != null, actor.editor_id + " retains NIF specular settings")
                print("R4 material actor=", actor.editor_id, " material=", material)
                for vertex in vertices.size():
                    var point := Vector3.ZERO
                    for influence in 4:
                        var index := vertex * 4 + influence
                        point += (observation.poses[offset + bones[index]] * vertices[vertex]) * weights[index]
                    point = placement * point
                    bounds = AABB(point, Vector3.ZERO) if first else bounds.expand(point)
                    first = false
            offset += count
        if first:
            continue
        checked += 1
        var head_material: ShaderMaterial
        for shape: Dictionary in observation.shapes:
            if shape.head_part_type == 1:
                head_material = shape.mesh.surface_get_material(0)
        if not OS.get_environment("SOE_FACE_SHADER").is_empty():
            var control := Shader.new()
            var bones: Array = head_material.get_shader_parameter("skin_normals")
            control.code = FileAccess.get_file_as_string(OS.get_environment("SOE_FACE_SHADER")).replace("CULL_MODE", "cull_back").replace("BONE_COUNT", str(bones.size())).replace("FACE_SPECULAR_ENABLED", "1" if head_material.get_shader_parameter("model_space_normals") else "0")
            head_material.shader = control
        var measurement_shader := Shader.new()
        measurement_shader.code = head_material.shader.code.replace("void fragment() {", "uniform bool r4_hide_head = false;\nvoid fragment() {\n    if (r4_hide_head) { discard; }")
        head_material.shader = measurement_shader
        for key in ["diffuse_map", "tint_map", "detail_map", "normal_map"]:
            var texture: Texture2D = head_material.get_shader_parameter(key)
            var tex_image := texture.get_image()
            if tex_image.is_compressed():
                tex_image.decompress()
            print("R4 texture ", key, " size=", tex_image.get_size(), " pixel=", tex_image.get_pixel(tex_image.get_width()/2, tex_image.get_height()/2))
        var center := bounds.get_center()
        camera.global_position = center + placement.basis.orthonormalized() * Vector3(0.0, 0.0, -bounds.size.length() * 1.8)
        camera.look_at(center)
        await process_frame
        await process_frame
        await RenderingServer.frame_post_draw
        var image := root.get_texture().get_image()
        _check(not head_rids.is_empty(), "head surface has a live RID")
        head_material.set_shader_parameter("r4_hide_head", true)
        await process_frame
        await process_frame
        await RenderingServer.frame_post_draw
        var without_head := root.get_texture().get_image()
        head_material.set_shader_parameter("r4_hide_head", false)
        var warm := 0
        var sum := Vector3.ZERO
        var pixels := 0
        var low := camera.unproject_position(bounds.position)
        var high := low
        for corner in 8:
            var point := bounds.position + bounds.size * Vector3(float(corner & 1), float((corner >> 1) & 1), float((corner >> 2) & 1))
            var screen := camera.unproject_position(point)
            low = low.min(screen)
            high = high.max(screen)
        var margin := (high - low) * 0.35
        low += margin
        high -= margin
        for y in range(maxi(0, int(low.y)), mini(image.get_height(), int(high.y))):
            for x in range(maxi(0, int(low.x)), mini(image.get_width(), int(high.x))):
                var pixel := image.get_pixel(x, y)
                var control := without_head.get_pixel(x, y)
                if absf(pixel.r-control.r) + absf(pixel.g-control.g) + absf(pixel.b-control.b) < 0.02:
                    continue
                sum += Vector3(pixel.r, pixel.g, pixel.b)
                pixels += 1
                if pixel.r - pixel.b > 0.03 and pixel.r > pixel.b * 1.15:
                    warm += 1
        var mean := sum / maxi(pixels, 1)
        _check(warm >= 500 and mean.x - mean.z > 0.03, actor.editor_id + " warm=" + str(warm) + " mean=" + str(mean))
        if actor.editor_id == "LuciaRef":
            _check(mean.y >= mean.x * 0.5, "child retains the baseline skin hue")
        print("R4 face actor=", actor.editor_id, " warm=", warm, " mean=", mean)
        image.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("face_" + str(actor.object) + ".png"))
    _check(checked >= 2, "at least two resident faces sampled")
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 streamed face failures=", failures)
    quit(0 if failures == 0 else 1)

func _process(_delta: float) -> bool:
    return false
