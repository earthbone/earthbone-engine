# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Fixed before baseline: production and authored hair colour differ on at most
## three percent of covered pixels, over three adults in each loaded location.
var failures := 0
var exterior := false

func _initialize() -> void:
    exterior = OS.get_environment("SOE_HAIR_EXTERIOR") == "1"
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse", false)

func _check(ok: bool, label: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 hair: ", label)

func _capture() -> Image:
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    return root.get_texture().get_image()

func _points(observation: Dictionary) -> Array:
    var result := []
    var offset := 0
    for shape: Dictionary in observation.shapes:
        var arrays: Array = shape.mesh.surface_get_arrays(0)
        var bones: PackedInt32Array = arrays[Mesh.ARRAY_BONES]
        var weights: PackedFloat32Array = arrays[Mesh.ARRAY_WEIGHTS]
        var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
        var count := 0
        for bone in bones:
            count = maxi(count, bone + 1)
        var points := PackedVector3Array()
        for vertex in vertices.size():
            var point := vertices[vertex] if bones.is_empty() else Vector3.ZERO
            if not bones.is_empty():
                for influence in 4:
                    var index := vertex * 4 + influence
                    point += (observation.poses[offset + bones[index]] * vertices[vertex]) * weights[index]
            points.append(observation.placement * point)
        result.append(points)
        offset += count
    return result

func _run() -> void:
    _check(OS.get_environment("DISPLAY") == ":99", "private display")
    await super._start()
    if not loaded:
        quit(1)
        return
    if exterior:
        var exited := false
        for door: Dictionary in player.call("gameplay_doors"):
            if not door.locked:
                exited = player.call("activate_reference", door.file, door.object)
                break
        _check(exited, "inn exit into Whiterun exterior")
        for frame in 1800:
            await process_frame
            if frame > 90 and not player.call("is_load_door_pending") and not player.call("is_location_preparing"):
                break
        _check(player.call("gameplay_location_ready"), "Whiterun exterior ready")
    for frame in 600:
        await process_frame
        var counts: Dictionary = player.call("npc_visual_counts")
        if counts.drawn >= 3 and counts.pending == 0:
            break
    player.set_process(false)
    player.set_physics_process(false)
    streamer.set_process(false)
    for visual in [player_body,player_arms]:
        visual.set_process(false)
    for node in root.find_children("*","Node",true,false):
        var script: Script = node.get_script()
        if script != null and script.resource_path == "res://record_sky.gd":
            node.set_process(false)
    Engine.time_scale = 0.0
    for overlay in root.find_children("*", "CanvasLayer", true, false):
        overlay.hide()
    var actors: Array = player.call("npc_visuals")
    print("R4 HAIR residents=", actors)
    for actor: Dictionary in actors:
        if actor.node_id != 0:
            var resident := instance_from_id(actor.node_id)
            resident.set("playing", false)
            resident.set_process(false)
    var walk_camera := root.get_camera_3d()
    var camera := Camera3D.new()
    camera.environment = walk_camera.environment
    var exposure := CameraAttributesPractical.new()
    exposure.auto_exposure_enabled = false
    exposure.exposure_multiplier = 1.0 if walk_camera.attributes == null else walk_camera.attributes.exposure_multiplier
    camera.attributes = exposure
    player.get_parent().add_child(camera)
    camera.near = 0.01
    camera.fov = 30.0
    camera.current = true
    var oracle: Dictionary = JSON.parse_string(FileAccess.get_file_as_string("res://../target/r4-hair-reference.json"))
    var sampled := 0
    for actor: Dictionary in actors:
        if actor.node_id == 0 or actor.editor_id.contains("Lucia") or actor.editor_id.contains("Braith"):
            continue
        var node := instance_from_id(actor.node_id)
        var observation: Dictionary = node.call("play_path_observation")
        var bounds := AABB()
        var first := true
        var hairs := []
        var heads := []
        var points := _points(observation)
        for i in observation.shapes.size():
            var shape: Dictionary = observation.shapes[i]
            if shape.head_part_type == 3 or str(shape.name).to_lower().contains("hairline"):
                hairs.append(shape)
            if shape.head_part_type == 1:
                heads.append(shape)
                for point in points[i]:
                    bounds = AABB(point, Vector3.ZERO) if first else bounds.expand(point)
                    first = false
        if hairs.is_empty() or first:
            continue
        sampled += 1
        print("R4 HAIR actor=", actor.editor_id, " heads=", heads.map(func(s): return s.name), " hair=", hairs.map(func(s): return s.name), " placement=", observation.placement)
        for angle in [0.0, -0.6, 0.6]:
            var basis: Basis = observation.placement.basis.orthonormalized()
            var center := bounds.get_center()
            camera.global_position = center + basis * Vector3(sin(angle), 0.2, -cos(angle)) * bounds.size.length() * 1.8
            camera.look_at(center)
            for frame in 60:
                await process_frame
            var normal: Image = await _capture()
            var repeated: Image = await _capture()
            var unchanged_diff := 0
            for y in normal.get_height():
                for x in normal.get_width():
                    var a := normal.get_pixel(x,y)
                    var b := repeated.get_pixel(x,y)
                    unchanged_diff += int(absf(a.r-b.r)+absf(a.g-b.g)+absf(a.b-b.b) > 0.03)
            print("R4 HAIR unchanged actor=",actor.editor_id," angle=",angle," differing_pixels=",unchanged_diff)
            var prefix: String = ("outside_" if exterior else "inn_") + actor.editor_id + "_" + str(angle)
            normal.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join(prefix + ".png"))
            for shape: Dictionary in hairs:
                RenderingServer.instance_set_visible(shape.rid, false)
            var hidden: Image = await _capture()
            hidden.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join(prefix + "_no_hair.png"))
            for shape: Dictionary in hairs:
                RenderingServer.instance_set_visible(shape.rid, true)
            var originals := []
            for shape: Dictionary in hairs:
                var key: String = str(actor.file) + ":" + str(actor.object) + ":" + str(shape.name)
                _check(oracle.has(key), "authored oracle for " + key)
                if not oracle.has(key):
                    continue
                var arrays: Array = shape.mesh.surface_get_arrays(0)
                var material: Material = shape.mesh.surface_get_material(0)
                var actual_colors: PackedColorArray = arrays[Mesh.ARRAY_COLOR]
                var color_error := 0.0
                for color_index in actual_colors.size():
                    for channel in 4:
                        color_error = maxf(color_error,absf(actual_colors[color_index][channel]-oracle[key][color_index][channel]))
                print("R4 HAIR array=",key," max_color_error=",color_error," material=",material.get_class()," vertex_srgb=",material.vertex_color_is_srgb if material is StandardMaterial3D else "shader")
                originals.append({"shape": shape, "arrays": arrays.duplicate(true), "material": material})
                var colors := PackedColorArray()
                for color in oracle[key]:
                    colors.append(Color(color[0], color[1], color[2], color[3]))
                arrays[Mesh.ARRAY_COLOR] = colors
                shape.mesh.clear_surfaces()
                shape.mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
                var control_material: Material = material.duplicate()
                if control_material is StandardMaterial3D:
                    control_material.albedo_color = Color.WHITE
                    control_material.vertex_color_is_srgb = true
                elif control_material is ShaderMaterial and not control_material.shader.code.contains("vertex_color_to_linear"):
                    var shader := Shader.new()
                    shader.code = control_material.shader.code.replace("void fragment() {", "vec3 vertex_color_to_linear(vec3 color) { return mix(pow((color + vec3(0.055)) / 1.055, vec3(2.4)), color / 12.92, lessThan(color, vec3(0.04045))); }\nvoid fragment() {").replace("vec4(COLOR.rgb,", "vec4(vertex_color_to_linear(COLOR.rgb),")
                    control_material.shader = shader
                shape.mesh.surface_set_material(0, control_material)
            var reference: Image = await _capture()
            reference.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join(prefix + "_reference.png"))
            var changed := 0
            var covered := 0
            var screen_low := Vector2(INF, INF)
            var screen_high := -screen_low
            for point in points:
                for position in point:
                    var projected := camera.unproject_position(position)
                    screen_low = screen_low.min(projected)
                    screen_high = screen_high.max(projected)
            for y in range(maxi(0, int(screen_low.y)), mini(normal.get_height(), int(screen_high.y) + 1)):
                for x in range(maxi(0, int(screen_low.x)), mini(normal.get_width(), int(screen_high.x) + 1)):
                    var a := normal.get_pixel(x,y)
                    var b := hidden.get_pixel(x,y)
                    var c := reference.get_pixel(x,y)
                    var production_diff := absf(a.r-b.r)+absf(a.g-b.g)+absf(a.b-b.b)
                    var reference_diff := absf(c.r-b.r)+absf(c.g-b.g)+absf(c.b-b.b)
                    if maxf(production_diff,reference_diff) > 0.03:
                        covered += 1
                        changed += int(absf(a.r-c.r)+absf(a.g-c.g)+absf(a.b-c.b) > 0.03)
            print("R4 HAIR pixels=", prefix, " covered=", covered, " mismatched=", changed)
            _check(unchanged_diff <= covered * 0.03, prefix + " unchanged frame calibration")
            _check(covered >= 1000, "hair contributes at least 1000 pixels")
            _check(changed <= covered * 0.03, prefix + " authored hair colour mismatch " + str(changed) + "/" + str(covered))
            for original: Dictionary in originals:
                original.shape.mesh.clear_surfaces()
                original.shape.mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, original.arrays)
                original.shape.mesh.surface_set_material(0, original.material)
            print("R4 HAIR captured=", prefix)
        if sampled >= 4:
            break
    _check(sampled >= 3, "three adults sampled, got " + str(sampled))
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 hair failures=", failures)
    Engine.time_scale = 1.0
    quit(0 if failures == 0 else 1)

func _process(_delta: float) -> bool:
    return false
