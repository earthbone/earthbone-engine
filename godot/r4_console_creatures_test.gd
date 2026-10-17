extends "res://walkthrough.gd"

var failures := 0

func _configure_player() -> void:
    root.size = Vector2i(1280, 720)
    player.set("capture_mouse", false)

func _spawn_position(_streamer: Node3D) -> Vector3:
    return Vector3(-125.0, -800.0, 120.0)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("R4_CONSOLE FAIL: ", message)

func _click(point: Vector2) -> void:
    var event := InputEventMouseButton.new()
    event.button_index = MOUSE_BUTTON_LEFT
    event.position = point
    event.pressed = true
    Input.parse_input_event(event)
    Input.flush_buffered_events()
    event = event.duplicate()
    event.pressed = false
    Input.parse_input_event(event)
    Input.flush_buffered_events()

func _bring_reference(reference: String, origin: Vector3) -> bool:
    var parts := reference.split(":")
    if parts.size() != 2:
        return false
    var file := parts[0].to_int()
    var object := parts[1].hex_to_int()
    var runtime := "%08X" % ((file << 24) | object)
    var moved := str(player.call("console_execute", runtime + ".moveto player"))
    if moved.begins_with("error:"):
        return false
    player.call("console_execute", runtime + ".enable")
    player.call("console_execute", runtime + ".setpos x " + str(origin.x + 200.0))
    player.call("console_execute", runtime + ".setpos y " + str(origin.y))
    player.call("console_execute", runtime + ".setpos z " + str(origin.z))
    var pose: Dictionary = player.call("gameplay_reference_pose", file, object)
    if pose.is_empty() or not pose.visible:
        player.call("set_reference_enabled", file, object, false)
        return false
    player.call("sync_contact_actors")
    var body: RID = player.call("contact_actor_body", file, object)
    if not body.is_valid():
        player.call("set_reference_enabled", file, object, false)
        return false
    return true

func _upper_triangle(observation: Dictionary) -> PackedVector3Array:
    var best := PackedVector3Array()
    var highest := -INF
    var offset := 0
    for shape: Dictionary in observation.shapes:
        var arrays: Array = shape.mesh.surface_get_arrays(0)
        var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
        var bones: PackedInt32Array = arrays[Mesh.ARRAY_BONES]
        var weights: PackedFloat32Array = arrays[Mesh.ARRAY_WEIGHTS]
        var indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
        var count := 0
        for bone in bones:
            count = maxi(count, bone + 1)
        var posed := PackedVector3Array()
        for vertex in vertices.size():
            var point := Vector3.ZERO
            for influence in 4:
                var index := vertex * 4 + influence
                var pose: Transform3D = observation.poses[offset + bones[index]]
                point += (pose * vertices[vertex]) * weights[index]
            posed.append(observation.placement * point)
        for index in range(0, indices.size(), 3):
            var a := posed[indices[index]]
            var b := posed[indices[index + 1]]
            var c := posed[indices[index + 2]]
            var height := (a.y + b.y + c.y) / 3.0
            if height > highest and (b - a).cross(c - a).length_squared() > 0.00001:
                highest = height
                best = PackedVector3Array([a, b, c])
        offset += count
    return best

func _start() -> void:
    await super._start()
    if not loaded:
        return
    var pattern := RegEx.new()
    pattern.compile("\\(([0-9A-F]{8})\\)")
    for _frame in range(60):
        await physics_frame
    _check(player.call("is_on_ground"), "fixture starts on the main inn floor")
    var origin: Vector3 = player.call("position")
    player.call("console_execute", "tai")
    for kind in ["giant", "mammoth"]:
        var base := ""
        var spawned := ""
        var listing := str(player.call("console_execute", "help " + kind + " 4 NPC_"))
        for row in listing.split("\n"):
            var found := pattern.search(row)
            if found != null and row.to_lower().contains("enc" + kind):
                var candidate := found.get_string(1)
                var placed := str(player.call("console_execute", "findref " + candidate + " 64"))
                if placed.begins_with("error:"):
                    continue
                for reference in placed.split("\n"):
                    if _bring_reference(reference, origin):
                        base = candidate
                        spawned = reference
                        break
                if not base.is_empty():
                    break
        _check(not base.is_empty(), kind + " placed base exists")
        if base.is_empty():
            continue
        print("R4_CONSOLE spawn kind=", kind, " base=", base, " reference=", spawned)
        var parts := spawned.split(":")
        _check(parts.size() == 2, kind + " placed as a reference")
        if parts.size() != 2:
            continue
        var file := parts[0].to_int()
        var object := parts[1].hex_to_int()
        var runtime := "%08X" % ((file << 24) | object)
        player.call("console_execute", runtime + ".moveto player")
        player.call("console_execute", runtime + ".enable")
        print("R4_CONSOLE enabled=", player.call("console_execute", runtime + ".getdisabled"))
        player.call("console_execute", runtime + ".setpos x " + str(origin.x + 200.0))
        player.call("console_execute", runtime + ".setpos y " + str(origin.y))
        player.call("console_execute", runtime + ".setpos z " + str(origin.z))
        var visual: Node
        var deadline := Time.get_ticks_msec() + 60000
        while Time.get_ticks_msec() < deadline:
            await process_frame
            for row: Dictionary in player.call("npc_visuals"):
                if row.file == file and row.object == object and row.node_id != 0 and not row.pending:
                    visual = instance_from_id(row.node_id)
            if is_instance_valid(visual):
                break
        _check(is_instance_valid(visual), kind + " rendered in free walk")
        if not is_instance_valid(visual):
            print("R4_CONSOLE missing visual file=", file, " object=", object, " bodies=", player.call("contact_actor_bodies"), " visuals=", player.call("npc_visuals"))
            continue
        player.set("auto_step", false)
        visual.call("advance", 0.0)
        var observation: Dictionary = visual.call("play_path_observation")
        var triangle := _upper_triangle(observation)
        _check(triangle.size() == 3, kind + " posed triangle")
        if triangle.size() != 3:
            continue
        var center := (triangle[0] + triangle[1] + triangle[2]) / 3.0
        _check(center.y - observation.placement.origin.y > 2.0, kind + " click lies above controller")
        var normal := (triangle[1] - triangle[0]).cross(triangle[2] - triangle[0]).normalized()
        var camera := root.get_camera_3d()
        player.call("console_set_open", true)
        player.call("console_execute", "prid 14")
        camera.global_position = center + normal * 0.2
        camera.look_at(center, Vector3.FORWARD if absf(normal.dot(Vector3.UP)) > 0.95 else Vector3.UP)
        _click(root.get_visible_rect().size / 2.0)
        var selected := str(player.call("console_execute", "getselected"))
        _check(selected == spawned, kind + " body click selects " + spawned + ", saw " + selected)
        print("R4_CONSOLE kind=", kind, " reference=", spawned, " point=", center, " selected=", selected)
        player.call("console_set_open", false)
        player.call("set_reference_enabled", file, object, false)
        player.set("auto_step", true)
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4_CONSOLE failures=", failures)
    quit(0 if failures == 0 else 1)
