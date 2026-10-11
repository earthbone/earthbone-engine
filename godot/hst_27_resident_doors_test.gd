extends SceneTree

## HST-27 installed front doors retain their node and clear all collision paths.
## Thresholds: a full cycle per door, a blocked closed control, zero open
## contacts, less than 0.01 metres of capsule travel error and 100 changed
## capture pixels when the visible mesh swings or disappears.

var failures := 0
var target := ""
var framing_camera: Camera3D

func _leaf_at_hinge(state: Dictionary, placement: Transform3D, radius: float) -> void:
    var hinge := placement.origin
    _check(state.visible, "door geometry stays visible")
    var vertices: PackedVector3Array = state.get("world_vertices", PackedVector3Array())
    var maximum := 0.0
    for point: Vector3 in vertices:
        maximum = maxf(maximum, point.distance_to(hinge))
    _check(maximum <= radius + 0.01, "rendered leaf stays within its swing radius: " + str(maximum) + " <= " + str(radius))
    _check(not vertices.is_empty(), "inspect actual rendered vertices")
    print("hst_27 fraction=", state.fraction, " vertices=", vertices.size(), " hinge_distance=", maximum, " radius=", radius)

func _initialize() -> void:
    create_timer(240.0).timeout.connect(func():
        printerr("FAIL hst_27: scene timeout")
        quit(1))
    call_deferred("_hst_27_resident_doors")

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL hst_27: ", message)

func _physics_settle() -> void:
    await physics_frame
    await physics_frame
    await process_frame

func _capture(path: String) -> Image:
    framing_camera.current = true
    await process_frame
    await RenderingServer.frame_post_draw
    var image := get_root().get_texture().get_image()
    _check(image.save_png(path) == OK, "windowed door capture saves")
    return image

func _capture_leaf(path: String, state: Dictionary) -> Image:
    var doorway: Image = await _capture(path)
    var camera := get_root().get_camera_3d()
    var mask := camera.cull_mask
    camera.cull_mask = 1 << 19
    for instance: RID in state.instances:
        RenderingServer.instance_set_layer_mask(instance, 1 << 19)
    var drawn: Image = await _capture(path.get_basename() + "_leaf_only.png")
    for instance: RID in state.instances:
        RenderingServer.instance_set_visible(instance, false)
    var hidden: Image = await _capture(path.get_basename() + "_without_leaf.png")
    for instance: RID in state.instances:
        RenderingServer.instance_set_visible(instance, state.visible)
        RenderingServer.instance_set_layer_mask(instance, 1)
    camera.cull_mask = mask
    var pixels := _changed(drawn, hidden)
    print("hst_27 capture=", path, " leaf_pixels=", pixels)
    _check(pixels >= 100, "leaf contributes at least 100 visible pixels: " + path)
    return doorway

func _changed(a: Image, b: Image) -> int:
    var count := 0
    for y in a.get_height():
        for x in a.get_width():
            var first := a.get_pixel(x, y)
            var second := b.get_pixel(x, y)
            if max(abs(first.r - second.r), abs(first.g - second.g), abs(first.b - second.b)) > 0.05:
                count += 1
    return count

func _walk(capsule: CharacterBody3D, placement: Transform3D, bounds: AABB) -> Dictionary:
    # CONTEXT: The panel's thin horizontal axis is the doorway normal.
    var normal := Vector3.FORWARD if bounds.size.z < bounds.size.x else Vector3.RIGHT
    var center := bounds.get_center()
    var distance := 1.0
    capsule.global_position = placement * (center - normal * distance)
    await _physics_settle()
    # CONTEXT: Load-door interiors end behind the panel. Cross its plane with
    # the whole capsule, without extending the sweep into that end wall.
    var clearance := 0.21 / placement.basis.get_scale().x
    var motion := placement.basis * normal * (distance + clearance)
    var start := capsule.global_position
    var hit := capsule.move_and_collide(motion)
    return {"blocked": hit != null, "error": (capsule.global_position - start - motion).length(),
        "rid": hit.get_collider_rid() if hit != null else RID(),
        "point": hit.get_position() if hit != null else Vector3.ZERO,
        "normal": hit.get_normal() if hit != null else Vector3.ZERO,
        "start": start, "motion": motion}

func _hst_27_resident_doors() -> void:
    var data := OS.get_environment("SKYRIM_DATA")
    if data.is_empty():
        print("SKIP hst_27: SKYRIM_DATA unset")
        quit(0)
        return
    target = OS.get_environment("CARGO_TARGET_DIR")
    if target.is_empty():
        target = ProjectSettings.globalize_path("res://../target")
    var manifest = JSON.parse_string(FileAccess.get_file_as_string(target.path_join("hst_27_doors.json")))
    if not manifest is Array or not ClassDB.class_exists("PlayerNode"):
        printerr("FAIL hst_27: manifest or extension missing")
        quit(1)
        return
    var environment := Environment.new()
    environment.background_mode = Environment.BG_COLOR
    environment.background_color = Color(0.08, 0.1, 0.14)
    environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
    environment.ambient_light_color = Color.WHITE
    environment.ambient_light_energy = 0.8
    var world_environment := WorldEnvironment.new()
    world_environment.environment = environment
    get_root().add_child(world_environment)
    var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
    get_root().add_child(streamer)
    _check(streamer.call("load_data", data), "installed data loads")
    streamer.set_process(false)
    var player: Node3D = ClassDB.instantiate("PlayerNode")
    player.set("capture_mouse", false)
    player.set("auto_step", false)
    get_root().add_child(player)
    var capsule := CharacterBody3D.new()
    capsule.collision_layer = 2
    capsule.collision_mask = 1
    var shape := CollisionShape3D.new()
    var geometry := CapsuleShape3D.new()
    geometry.radius = 0.2
    geometry.height = 1.4
    shape.shape = geometry
    capsule.add_child(shape)
    get_root().add_child(capsule)
    var camera := Camera3D.new()
    framing_camera = camera
    get_root().add_child(camera)
    camera.current = true
    var captures := target.path_join("hst_27")
    _check(DirAccess.make_dir_recursive_absolute(captures) == OK, "capture directory exists")
    for entry: Dictionary in manifest:
        var file := int(entry.file)
        var object := int(entry.object)
        _check(player.call("use_streamer_data", streamer), "player shares streamer's load order")
        _check(streamer.call("stream_cell", entry.cell) > 0, "interior attaches")
        var pos: Array = entry.position
        _check(player.call("enter_cell", entry.cell, pos[0], pos[1], float(pos[2]) + 1024.0) > 0, "player collision attaches")
        _check(player.call("start_gameplay"), "gameplay starts")
        player.call("advance", 0.2)
        _check(streamer.call("stream_cell_collision", entry.cell) > 0, "streamer collision attaches")
        player.call("advance", 0.0)
        if entry.cell == "WhiterunBanneredMare":
            var exercised_swing := false
            for swing: Dictionary in player.call("gameplay_swing_doors"):
                var state: Dictionary = streamer.call("door_state", int(swing.file), int(swing.object))
                if state.is_empty() or not state.visible:
                    continue
                var hinge: Transform3D = state.placement_transform
                var leaf_bounds: AABB = state.local_bounds
                var radius_key := "%d:%d" % [int(swing.file), int(swing.object)]
                if not entry.swing_radii.has(radius_key):
                    continue
                var swing_radius := float(entry.swing_radii[radius_key])
                var leaf_center := hinge * leaf_bounds.get_center()
                var leaf_normal := Vector3.FORWARD if leaf_bounds.size.z < leaf_bounds.size.x else Vector3.RIGHT
                camera.global_position = leaf_center - hinge.basis * leaf_normal * 3.0
                camera.look_at(leaf_center)
                _check(player.call("set_reference_open", int(swing.file), int(swing.object), false), "close swing fixture")
                player.call("advance", 0.0)
                streamer.call("advance_doors", 1.0)
                _leaf_at_hinge(streamer.call("door_state", int(swing.file), int(swing.object)), hinge, swing_radius)
                await _capture_leaf(captures.path_join("house_swing_closed.png"), streamer.call("door_state", int(swing.file), int(swing.object)))
                _check(player.call("activate_reference", int(swing.file), int(swing.object)), "activate house swing door")
                streamer.call("advance_doors", 0.5)
                var swing_opening: Dictionary = streamer.call("door_state", int(swing.file), int(swing.object))
                _check(swing_opening.opened and is_equal_approx(swing_opening.fraction, 0.5), "activation reaches the opening pose")
                _leaf_at_hinge(swing_opening, hinge, swing_radius)
                _check(streamer.call("rendered_reference_pose", int(swing.file), int(swing.object)).get("visible", false), "activation keeps presentation visible")
                await _capture_leaf(captures.path_join("house_swing_opening.png"), streamer.call("door_state", int(swing.file), int(swing.object)))
                streamer.call("advance_doors", 1.0)
                var swing_open: Dictionary = streamer.call("door_state", int(swing.file), int(swing.object))
                _check(swing_open.fraction == 1.0, "activation reaches the open pose")
                _leaf_at_hinge(swing_open, hinge, swing_radius)
                await _capture_leaf(captures.path_join("house_swing_open.png"), streamer.call("door_state", int(swing.file), int(swing.object)))
                _check(player.call("activate_reference", int(swing.file), int(swing.object)), "activate house swing door to close")
                streamer.call("advance_doors", 0.5)
                var swing_closing: Dictionary = streamer.call("door_state", int(swing.file), int(swing.object))
                _check(not swing_closing.opened and is_equal_approx(swing_closing.fraction, 0.5), "activation reaches the closing pose")
                _leaf_at_hinge(swing_closing, hinge, swing_radius)
                await _capture_leaf(captures.path_join("house_swing_closing.png"), streamer.call("door_state", int(swing.file), int(swing.object)))
                exercised_swing = true
                break
            _check(exercised_swing, "installed house includes an activated swing door")
        var attached: Dictionary = streamer.call("door_state", file, object)
        _check(not attached.is_empty(), "front door has a node on cell attach")
        _check(streamer.call("rendered_reference_pose", file, object).get("visible", false), "cell attach retains door presentation")
        if attached.is_empty():
            continue
        var identity := int(attached.node)
        var children := streamer.get_child_count()
        var bounds: AABB = attached.local_bounds
        var placement: Transform3D = attached.placement_transform
        print("hst_27 portal cell=", entry.cell, " bounds=", bounds, " placement=", placement)
        var radius := float(entry.radius)
        var center: Vector3 = placement * bounds.get_center()
        var normal := Vector3.FORWARD if bounds.size.z < bounds.size.x else Vector3.RIGHT
        camera.global_position = center - placement.basis * normal * 4.0 + Vector3.UP * 0.3
        camera.look_at(center)
        camera.current = true
        var closed_image: Image
        var open_image: Image
        for cycle in 1:
            _check(player.call("set_reference_open", file, object, false), "close World door")
            player.call("advance", 0.0)
            streamer.call("advance_doors", 1.0)
            await _physics_settle()
            var closed: Dictionary = streamer.call("door_state", file, object)
            _leaf_at_hinge(closed, placement, radius)
            _check(int(closed.node) == identity and closed.fraction == 0.0 and closed.visible, "close keeps the original visible node")
            var bodies: Dictionary = player.call("reference_collision_bodies", file, object)
            _check(bodies.movement > 0 and bodies.query > 0, "closed door blocks both player collision paths")
            _check(streamer.call("reference_collision_bodies", file, object) > 0, "closed door blocks streamer physics")
            var blocked: Dictionary = await _walk(capsule, placement, bounds)
            _check(blocked.blocked, "closed doorway blocks a real host capsule")
            if cycle == 0:
                closed_image = await _capture_leaf(captures.path_join(entry.cell + "_closed.png"), closed)
            _check(player.call("set_reference_open", file, object, true), "open World door")
            player.call("advance", 0.0)
            streamer.call("advance_doors", 0.5)
            var midway: Dictionary = streamer.call("door_state", file, object)
            _leaf_at_hinge(midway, placement, radius)
            _check(streamer.call("rendered_reference_pose", file, object).get("visible", false), "opening keeps reference presentation visible")
            if cycle == 0:
                await _capture_leaf(captures.path_join(entry.cell + "_opening.png"), midway)
            _check(midway.fraction >= 0.5 and midway.visible, "opening swings a visible mesh")
            _check(not placement.basis.is_equal_approx(midway.transform.basis), "actual door basis swings")
            streamer.call("advance_doors", 1.0)
            await _physics_settle()
            var opened: Dictionary = streamer.call("door_state", file, object)
            _leaf_at_hinge(opened, placement, radius)
            _check(int(opened.node) == identity and opened.fraction == 1.0 and opened.visible, "open retains visible node identity")
            _check(opened.surfaces == attached.surfaces and opened.instances == attached.instances and streamer.get_child_count() == children, "no geometry or node is rebuilt")
            bodies = player.call("reference_collision_bodies", file, object)
            _check(bodies.movement == 0 and bodies.query == 0, "open door has no player movement or query wall")
            _check(streamer.call("reference_collision_bodies", file, object) == 0, "open door has no streamer wall")
            var walked: Dictionary = await _walk(capsule, placement, bounds)
            _check(not walked.blocked and walked.error < 0.01, "capsule walks through open doorway: " + str(walked))
            if cycle == 0:
                open_image = await _capture_leaf(captures.path_join(entry.cell + "_open.png"), opened)
                _check(_changed(closed_image, open_image) >= 100, "actual door geometry visibly swings")
        _check(player.call("set_reference_open", file, object, false), "start closing")
        player.call("advance", 0.0)
        streamer.call("advance_doors", 0.5)
        _leaf_at_hinge(streamer.call("door_state", file, object), placement, radius)
        await _capture_leaf(captures.path_join(entry.cell + "_closing.png"), streamer.call("door_state", file, object))
        _check(player.call("set_reference_enabled", file, object, false), "disable door")
        player.call("advance", 0.0)
        await _physics_settle()
        var disabled: Dictionary = streamer.call("door_state", file, object)
        _check(int(disabled.node) == identity and not disabled.visible, "disable hides geometry and keeps node")
        var hidden: Node3D = instance_from_id(identity)
        _check(hidden != null and not hidden.visible, "actual control node hides")
        var disabled_image: Image = await _capture(captures.path_join(entry.cell + "_disabled.png"))
        _check(open_image != null and _changed(open_image, disabled_image) >= 100, "disable removes rendered door geometry")
        var bodies: Dictionary = player.call("reference_collision_bodies", file, object)
        _check(bodies.movement == 0 and bodies.query == 0 and streamer.call("reference_collision_bodies", file, object) == 0, "disable removes every collision path")
        _check(player.call("set_reference_open", file, object, false), "close disabled door")
        player.call("advance", 0.0)
        streamer.call("advance_doors", 1.0)
        bodies = player.call("reference_collision_bodies", file, object)
        _check(bodies.movement == 0 and bodies.query == 0, "closing disabled door keeps collision absent")
        _check(player.call("set_reference_enabled", file, object, true), "enable door")
        player.call("advance", 0.0)
        var restored: Dictionary = streamer.call("door_state", file, object)
        _check(int(restored.node) == identity and restored.visible, "enable restores the same door node")
        bodies = player.call("reference_collision_bodies", file, object)
        _check(bodies.movement > 0 and bodies.query > 0, "enable closed door restores collision")
        _check(player.call("set_reference_open", file, object, true), "unlock load door")
        _check(player.call("activate_reference", file, object), "activate interior load door")
        player.call("advance", 0.0)
        player.call("advance", 0.25)
        _check(not player.call("door_fade_active"), "load-door fade finishes before capture")
        await process_frame
        var outside: Dictionary = streamer.call("door_state", int(entry.far_file), int(entry.far_object))
        _check(not outside.is_empty() and outside.visible, "destination leaf is drawn")
        if not outside.is_empty():
            var outside_placement: Transform3D = outside.placement_transform
            var outside_bounds: AABB = outside.local_bounds
            var outside_radius := float(entry.far_radius)
            var outside_center := outside_placement * outside_bounds.get_center()
            var outside_normal := Vector3.FORWARD if outside_bounds.size.z < outside_bounds.size.x else Vector3.RIGHT
            camera.global_position = outside_center - outside_placement.basis * outside_normal * 4.0 + Vector3.UP * 0.3
            camera.look_at(outside_center)
            for stage: String in ["closed", "opening", "open", "closing"]:
                if stage == "closed":
                    _check(player.call("set_reference_open", int(entry.far_file), int(entry.far_object), false), "close exterior house door")
                elif stage == "opening":
                    _check(player.call("set_reference_open", int(entry.far_file), int(entry.far_object), true), "open exterior house door")
                elif stage == "closing":
                    _check(player.call("set_reference_open", int(entry.far_file), int(entry.far_object), false), "close exterior house door")
                player.call("advance", 0.0)
                streamer.call("advance_doors", 0.5 if stage in ["opening", "closing"] else 1.0)
                var exterior_state: Dictionary = streamer.call("door_state", int(entry.far_file), int(entry.far_object))
                _leaf_at_hinge(exterior_state, outside_placement, outside_radius)
                await _capture_leaf(captures.path_join(entry.cell + "_exterior_" + stage + ".png"), exterior_state)
        _check(player.call("set_reference_open", int(entry.far_file), int(entry.far_object), true), "unlock return door")
        _check(player.call("activate_reference", int(entry.far_file), int(entry.far_object)), "activate exterior house door and return")
        player.call("advance", 0.0)
        player.call("advance", 0.25)
        _check(not player.call("door_fade_active"), "load-door fade finishes before capture")
        await process_frame
        var returned: Dictionary = streamer.call("door_state", file, object)
        _check(not returned.is_empty(), "load door rebuilds on return")
        if not returned.is_empty():
            camera.global_position = center - placement.basis * normal * 4.0 + Vector3.UP * 0.3
            camera.look_at(center)
            _leaf_at_hinge(returned, placement, radius)
            await _capture_leaf(captures.path_join(entry.cell + "_returned.png"), returned)
        var sample_start := Time.get_ticks_usec()
        for sample in 1000:
            streamer.call("advance_doors", 0.0)
        var update_us := float(Time.get_ticks_usec() - sample_start) / 1000.0
        print("hst_27 cell=", entry.cell, " cycles=1 node=", identity, " surfaces=", attached.surfaces,
            " resident_nodes=", streamer.call("door_count"), " steady_advance_us=", update_us)
        streamer.call("clear")
        _check(streamer.call("door_state", file, object).is_empty() and not is_instance_id_valid(identity), "cell detach releases the node")
    capsule.free()
    player.free()
    streamer.free()
    camera.free()
    world_environment.free()
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " hst_27_resident_doors")
    quit(0 if failures == 0 else 1)
