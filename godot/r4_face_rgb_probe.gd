# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://r4_hair_probe.gd"

## Fixed before first run: four heads, four authored texture samples each,
## 500 head pixels and neutral mean RGB within 2/255 per channel.
## The blend follows local Community Shaders Lighting.hlsl GetFacegenBaseColor.
var rgb_samples := []

func _check(ok: bool, label: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 RGB: ", label)

func _texture_image(material: ShaderMaterial, name: String) -> Image:
    var texture: Texture2D = material.get_shader_parameter(name)
    var result := texture.get_image()
    if result.is_compressed():
        _check(result.decompress() == OK, name + " decompresses")
    return result

func _pixel(image: Image, uv: Vector2) -> Color:
    return image.get_pixel(int(uv.x*image.get_width()), int(uv.y*image.get_height()))

func _expected(base: Image, tint: Image, detail: Image, uv: Vector2) -> Color:
    var b := _pixel(base, uv)
    var t := _pixel(tint, uv)
    var d := Color.WHITE if detail == null else _pixel(detail, uv)
    var linear := Vector3.ZERO
    for channel in 3:
        var factor: float = 1.0 if detail == null else 3.984375*(d[channel]+(0.0 if channel == 1 else 1.0/255.0))
        var color: float = (b[channel]*b[channel]+2.0*t[channel]*b[channel]*(1.0-b[channel]))*factor
        linear[channel] = clampf(pow(maxf(color,0.0),2.2),0.0,1.0)
    return Color(linear.x,linear.y,linear.z).linear_to_srgb()

func _run() -> void:
    _check(OS.get_environment("DISPLAY") == ":99", "private display")
    await super._start()
    if not loaded:
        quit(1)
        return
    for frame in 600:
        await process_frame
        var counts: Dictionary = player.call("npc_visual_counts")
        if counts.drawn >= 4 and counts.pending == 0:
            break
    player.set_process(false)
    player.set_physics_process(false)
    streamer.set_process(false)
    for overlay in root.find_children("*", "CanvasLayer", true, false):
        overlay.hide()
    var actors: Array = player.call("npc_visuals")
    for actor: Dictionary in actors:
        if actor.node_id != 0:
            var node := instance_from_id(actor.node_id)
            node.set("playing", false)
            node.set_process(false)
    var camera := Camera3D.new()
    player.get_parent().add_child(camera)
    camera.near = 0.01
    camera.fov = 30.0
    var neutral := Environment.new()
    neutral.background_mode = Environment.BG_COLOR
    neutral.background_color = Color(0.0,0.15,0.0)
    neutral.tonemap_mode = Environment.TONE_MAPPER_LINEAR
    camera.environment = neutral
    var exposure := CameraAttributesPractical.new()
    exposure.auto_exposure_enabled = false
    exposure.exposure_multiplier = 1.0
    camera.attributes = exposure
    camera.current = true
    var heads := 0
    for actor: Dictionary in actors:
        if actor.node_id == 0 or actor.editor_id not in ["HuldaREF", "MikaelREF", "SaadiaRef", "LuciaRef"]:
            continue
        var node := instance_from_id(actor.node_id)
        var observation: Dictionary = node.call("play_path_observation")
        var points := _points(observation)
        for index in observation.shapes.size():
            var shape: Dictionary = observation.shapes[index]
            if shape.head_part_type != 1:
                continue
            heads += 1
            var hidden_parts := []
            for part: Dictionary in observation.shapes:
                if part.rid != shape.rid and part.visible:
                    hidden_parts.append(part.rid)
                    RenderingServer.instance_set_visible(part.rid,false)
            var material: ShaderMaterial = shape.mesh.surface_get_material(0)
            var base := _texture_image(material, "diffuse_map")
            var tint := _texture_image(material, "tint_map")
            var detail: Image = _texture_image(material, "detail_map") if material.get_shader_parameter("has_detail") else null
            var old_shader := material.shader
            var code := old_shader.code.replace("render_mode ", "render_mode unshaded, ")
            code = code.replace("void fragment() {", "uniform vec2 audit_uv = vec2(0.5);\nuniform bool audit_hide = false;\nvoid fragment() { if (audit_hide) { discard; }")
            code = code.replace(": hint_default_white;", ": hint_default_white, filter_nearest;")
            code = code.replace("uniform sampler2D detail_map;", "uniform sampler2D detail_map : filter_nearest;")
            for name in ["diffuse_map", "tint_map", "detail_map"]:
                code = code.replace("texture("+name+", UV)", "textureLod("+name+", audit_uv, 0.0)")
            var shader := Shader.new()
            shader.code = code
            material.shader = shader
            var bounds := AABB(points[index][0],Vector3.ZERO)
            for point in points[index]:
                bounds = bounds.expand(point)
            camera.global_position = bounds.get_center()+observation.placement.basis.orthonormalized()*Vector3(0.0,0.0,-bounds.size.length()*1.8)
            camera.look_at(bounds.get_center())
            for uv in [Vector2(0.25,0.25), Vector2(0.5,0.5), Vector2(0.75,0.5), Vector2(0.5,0.75)]:
                material.set_shader_parameter("audit_uv",uv)
                var normal: Image = await _capture()
                material.set_shader_parameter("audit_hide",true)
                var hidden: Image = await _capture()
                material.set_shader_parameter("audit_hide",false)
                var expected := _expected(base,tint,detail,uv)
                var sum := Vector3.ZERO
                var count := 0
                for y in normal.get_height():
                    for x in normal.get_width():
                        var a := normal.get_pixel(x,y)
                        var b := hidden.get_pixel(x,y)
                        if absf(a.r-b.r)+absf(a.g-b.g)+absf(a.b-b.b) > 0.03:
                            sum += Vector3(a.r,a.g,a.b)
                            count += 1
                var mean := sum/maxi(1,count)
                var error := maxf(absf(mean.x-expected.r),maxf(absf(mean.y-expected.g),absf(mean.z-expected.b)))
                _check(count >= 500, actor.editor_id+" head pixels")
                _check(error <= 2.0/255.0, actor.editor_id+" neutral RGB parity")
                var row := {"actor": actor.editor_id, "uv": [uv.x,uv.y], "pixels": count, "expected": [expected.r,expected.g,expected.b], "measured": [mean.x,mean.y,mean.z], "max_error": error}
                rgb_samples.append(row)
                print("R4 RGB ", JSON.stringify(row))
                normal.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("rgb_"+actor.editor_id+"_"+str(uv.x)+"_"+str(uv.y)+".png"))
            material.shader = old_shader
            for rid: RID in hidden_parts:
                RenderingServer.instance_set_visible(rid,true)
    _check(heads == 4, "three adults and child control")
    var file := FileAccess.open(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("face_rgb.json"),FileAccess.WRITE)
    file.store_string(JSON.stringify(rgb_samples,"  "))
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 neutral FaceGen RGB")
    quit(0 if failures == 0 else 1)
