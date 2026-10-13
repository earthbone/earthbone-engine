extends SceneTree

## Fixed before the first run: 1000 colored face pixels, mean red above 0.15,
## zero first-person head surfaces and zero visible third-person surfaces.
var failures := 0
var view: SubViewport

func _initialize() -> void:
    call_deferred("_run")

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL character regression: ", message)

func _head_bounds(observation: Dictionary) -> AABB:
    var bounds := AABB()
    var first := true
    var offset := 0
    var placement: Transform3D = observation.placement
    for shape: Dictionary in observation.shapes:
        var mesh := shape.mesh as ArrayMesh
        var arrays := mesh.surface_get_arrays(0)
        var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
        var bones: PackedInt32Array = arrays[Mesh.ARRAY_BONES]
        var weights: PackedFloat32Array = arrays[Mesh.ARRAY_WEIGHTS]
        var count := 0
        for bone in bones:
            count = maxi(count, bone + 1)
        RenderingServer.instance_set_visible(shape.rid, shape.head_part_type == 1)
        if shape.head_part_type == 1:
            for vertex in vertices.size():
                var point := Vector3.ZERO
                for influence in 4:
                    var index := vertex * 4 + influence
                    var pose: Transform3D = observation.poses[offset + bones[index]]
                    point += (pose * vertices[vertex]) * weights[index]
                point = placement * point
                bounds = AABB(point, Vector3.ZERO) if first else bounds.expand(point)
                first = false
        offset += count
    return bounds

func _capture() -> Image:
    await process_frame
    await process_frame
    await RenderingServer.frame_post_draw
    return view.get_texture().get_image()

func _texture_center(texture: Texture2D) -> Vector3:
    var image := texture.get_image()
    if image.is_compressed():
        _check(image.decompress() == OK, "installed texture decodes for pixel reference")
    var sum := Vector3.ZERO
    for y in [image.get_height() / 2 - 1, image.get_height() / 2]:
        for x in [image.get_width() / 2 - 1, image.get_width() / 2]:
            var pixel := image.get_pixel(x, y)
            sum += Vector3(pixel.r, pixel.g, pixel.b)
    return sum / 4.0

func _display_channel(linear: float) -> float:
    return 12.92 * linear if linear <= 0.0031308 else 1.055 * pow(linear, 1.0 / 2.4) - 0.055

func _check_face_blend(observation: Dictionary, camera: Camera3D) -> void:
    var material: ShaderMaterial
    for shape: Dictionary in observation.shapes:
        RenderingServer.instance_set_visible(shape.rid, false)
        if shape.head_part_type == 1:
            material = shape.mesh.surface_get_material(0).duplicate() as ShaderMaterial
    _check(material != null, "exported face has its tint shader")
    if material == null:
        return
    var base := _texture_center(material.get_shader_parameter("diffuse_map"))
    var tint := _texture_center(material.get_shader_parameter("tint_map"))
    var detail := _texture_center(material.get_shader_parameter("detail_map"))
    detail = 3.984375 * (detail + Vector3(1.0 / 255.0, 0.0, 1.0 / 255.0))
    var color := (base * base + 2.0 * tint * base * (Vector3.ONE - base)) * detail
    var expected := Vector3(_display_channel(pow(color.x, 2.2)), _display_channel(pow(color.y, 2.2)), _display_channel(pow(color.z, 2.2)))
    var shader := Shader.new()
    shader.code = material.shader.code.replace("render_mode cull_back;", "render_mode unshaded, cull_disabled;").replace("texture(", "textureLod(").replace(", UV)", ", vec2(0.5), 0.0)")
    # CONTEXT: The unskinned color probe has no bone attributes.
    var vertex_begin := shader.code.find("void vertex()")
    var fragment_begin := shader.code.find("void fragment()")
    _check(vertex_begin >= 0 and fragment_begin > vertex_begin, "face shader has its vertex and fragment stages")
    if vertex_begin < 0 or fragment_begin <= vertex_begin:
        return
    shader.code = shader.code.left(vertex_begin) + "void vertex() { face_normal_to_view = mat3(1.0); }\n" + shader.code.substr(fragment_begin)
    material.shader = shader
    var probe := MeshInstance3D.new()
    probe.mesh = QuadMesh.new()
    probe.mesh.size = Vector2(2.0, 2.0)
    probe.material_override = material
    view.add_child(probe)
    camera.position = Vector3(0.0, 0.0, 1.5)
    camera.look_at(Vector3.ZERO)
    var rendered := await _capture()
    var pixel := rendered.get_pixel(320, 180)
    var actual := Vector3(pixel.r, pixel.g, pixel.b)
    var error := (actual - expected).abs()
    _check(maxf(error.x, maxf(error.y, error.z)) <= 0.04, "face blend matches authored texture pixels within 0.04: " + str([actual, expected]))
    print("CHARACTER face_blend actual=", actual, " expected=", expected)
    var broken := Shader.new()
    broken.code = "shader_type spatial; render_mode unshaded, cull_disabled; uniform sampler2D diffuse_map : source_color; uniform sampler2D tint_map : source_color; void fragment() { ALBEDO = textureLod(diffuse_map, vec2(0.5), 0.0).rgb * textureLod(tint_map, vec2(0.5), 0.0).rgb; }"
    material.shader = broken
    var control := await _capture()
    pixel = control.get_pixel(320, 180)
    error = (Vector3(pixel.r, pixel.g, pixel.b) - expected).abs()
    _check(maxf(error.x, maxf(error.y, error.z)) > 0.04, "pixel reference rejects the former multiplicative face shader")
    probe.free()

func _run() -> void:
    _check(OS.get_environment("DISPLAY") == ":99", "requires private Xvfb :99")
    _check(DisplayServer.get_name() != "headless", "requires a rendered viewport")
    var data := OS.get_environment("SKYRIM_DATA")
    var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
    root.add_child(streamer)
    _check(streamer.call("load_data", data), "installed data loads")
    var player: Node3D = ClassDB.instantiate("PlayerNode")
    player.set("capture_mouse", false)
    player.set("auto_step", false)
    root.add_child(player)
    _check(player.call("use_streamer_data", streamer), "shared source binds")
    _check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "inn enters")
    _check(player.call("start_gameplay"), "game starts")
    view = SubViewport.new()
    view.size = Vector2i(640, 360)
    view.own_world_3d = true
    view.render_target_update_mode = SubViewport.UPDATE_ALWAYS
    root.add_child(view)
    var face: Node3D = ClassDB.instantiate("SkinnedActorNode")
    face.set("playing", false)
    view.add_child(face)
    _check(player.call("bind_actor_appearance", face, 0, 0x1A66E), "Hulda appearance binds")
    face.call("advance", 0.0)
    var observation: Dictionary = face.call("play_path_observation")
    var bounds := _head_bounds(observation)
    _check(bounds.size.length() > 0.0, "exported head has posed geometry")
    var camera := Camera3D.new()
    view.add_child(camera)
    var placement: Transform3D = observation.placement
    var center := bounds.get_center()
    camera.position = center + placement.basis * Vector3(0.0, 0.0, -bounds.size.length() * 1.8)
    camera.look_at(center)
    camera.current = true
    var light := DirectionalLight3D.new()
    view.add_child(light)
    light.rotation = camera.rotation
    light.light_energy = 1.0
    var environment := WorldEnvironment.new()
    environment.environment = Environment.new()
    environment.environment.background_mode = Environment.BG_COLOR
    environment.environment.background_color = Color.BLACK
    environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
    environment.environment.ambient_light_color = Color.WHITE
    environment.environment.ambient_light_energy = 0.25
    view.add_child(environment)
    var image := await _capture()
    var colored := 0
    var red := 0.0
    for y in image.get_height():
        for x in image.get_width():
            var pixel := image.get_pixel(x, y)
            if pixel.r - pixel.b > 0.04 and pixel.r > pixel.b * 1.2:
                colored += 1
                red += pixel.r
    var mean_red := red / maxf(1.0, colored)
    _check(colored >= 1000 and mean_red > 0.15, "face pixels are colored and lit: " + str([colored, mean_red]))
    var target := OS.get_environment("SOE_CHARACTER_OUTPUT")
    _check(image.save_png(target.path_join("hulda_face.png")) == OK, "face capture writes")
    print("CHARACTER face colored_pixels=", colored, " mean_red=", mean_red)
    await _check_face_blend(observation, camera)

    var body: Node3D = ClassDB.instantiate("SkinnedActorNode")
    body.set("playing", false)
    player.add_child(body)
    _check(player.call("bind_actor_appearance", body, 0, 0x14), "third-person body binds")
    var arms: Node3D = ClassDB.instantiate("SkinnedActorNode")
    arms.set("playing", false)
    arms.set("first_person", true)
    player.add_child(arms)
    _check(player.call("bind_actor_appearance", arms, 0, 0x14), "first-person arms bind")
    player.call("advance", 1.0 / 60.0)
    for visual in [body, arms]:
        visual.call("advance", 0.0)
    _check(not player.call("is_third_person"), "first-person camera is active")
    var first: Dictionary = arms.call("play_path_observation")
    _check(not first.shapes.is_empty(), "first-person arms have geometry")
    for shape: Dictionary in first.shapes:
        _check(shape.head_part_type == -1, "no first-person head part: " + str(shape.name))
    for shape: Dictionary in body.call("play_path_observation").shapes:
        _check(not shape.visible, "third-person surface culled from first-person camera: " + str(shape.name))
    print("CHARACTER first_person shapes=", first.shapes.size(), " head_parts=0 third_person_visible=0")

    body.free()
    arms.free()
    face.free()
    view.free()
    player.free()
    streamer.free()
    await process_frame
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " character regression")
    quit(0 if failures == 0 else 1)
