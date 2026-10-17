extends "res://character_regression_test.gd"

## Fixed before the first run: each adult face retains its model-space normal
## flag and every uploaded normal matrix agrees with its skin pose within 0.00001.
func _run() -> void:
    _check(OS.get_environment("DISPLAY") == ":99", "requires private Xvfb :99")
    _check(DisplayServer.get_name() != "headless", "requires rendered viewport")
    var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
    root.add_child(streamer)
    _check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "installed data loads")
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
    var camera := Camera3D.new()
    view.add_child(camera)
    camera.current = true
    var light := DirectionalLight3D.new()
    view.add_child(light)
    light.light_energy = 1.0
    var environment := WorldEnvironment.new()
    environment.environment = Environment.new()
    environment.environment.background_mode = Environment.BG_COLOR
    environment.environment.background_color = Color.BLACK
    environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
    environment.environment.ambient_light_color = Color.WHITE
    environment.environment.ambient_light_energy = 0.25
    view.add_child(environment)
    for entry in OS.get_environment("SOE_FACE_ACTORS").split(","):
        var fields := entry.split(":")
        var face: Node3D = ClassDB.instantiate("SkinnedActorNode")
        face.set("playing", false)
        view.add_child(face)
        _check(player.call("bind_actor_appearance", face, int(fields[1]), int(fields[2])), fields[0] + " binds")
        for elapsed in [0.0, 0.8]:
            face.call("advance", elapsed)
            var observation: Dictionary = face.call("play_path_observation")
            var offset := 0
            var checked := 0
            var changed_normals := 0
            for shape: Dictionary in observation.shapes:
                var material := shape.mesh.surface_get_material(0) as ShaderMaterial
                var count := 0
                for bone in shape.mesh.surface_get_arrays(0)[Mesh.ARRAY_BONES]:
                    count = maxi(count, bone + 1)
                if shape.head_part_type == 1 and material != null and material.get_shader_parameter("model_space_normals"):
                    var normals: Array = material.get_shader_parameter("skin_normals")
                    _check(normals.size() == count, fields[0] + " has one normal matrix per skin bone")
                    for bone in mini(count, normals.size()):
                        var expected: Basis = observation.poses[offset + bone].basis.inverse().transposed()
                        for axis in 3:
                            _check(normals[bone][axis].distance_to(expected[axis]) <= 0.00001, fields[0] + " normal matrix follows uploaded skin pose")
                            if expected[axis].distance_to(Basis.IDENTITY[axis]) > 0.00001:
                                changed_normals += 1
                    checked += 1
                offset += count
            if fields[0] != "Braith":
                _check(checked > 0, fields[0] + " exercises adult model-space face normals")
                _check(changed_normals > 0, fields[0] + " rejects the former identity normal matrices")
            var bounds := _head_bounds(observation)
            _check(bounds.size.length() > 0.0, fields[0] + " has head geometry")
            var placement: Transform3D = observation.placement
            var center := bounds.get_center()
            camera.position = center + placement.basis * Vector3(0.0, 0.0, -bounds.size.length() * 1.8)
            camera.look_at(center)
            light.rotation = camera.rotation
            var image := await _capture()
            var colored := 0
            for y in image.get_height():
                for x in image.get_width():
                    var pixel := image.get_pixel(x, y)
                    if pixel.r - pixel.b > 0.04 and pixel.r > pixel.b * 1.2:
                        colored += 1
            _check(colored >= 1000, fields[0] + " face remains colored: " + str(colored))
            var filename := fields[0] + "_" + str(elapsed) + ".png"
            _check(image.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join(filename)) == OK, "face capture writes")
            print("ADULT_FACE name=", fields[0], " normal_surfaces=", checked, " colored_pixels=", colored)
        face.free()
    view.free()
    player.free()
    streamer.free()
    await process_frame
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " adult face")
    quit(0 if failures == 0 else 1)
