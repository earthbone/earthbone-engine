extends "res://walkthrough.gd"

## HST-23 thresholds declared before capture: every enabled resident ACHR draws,
## target rigs have bones and head parts, at least 100 target pixels change,
## runtime/render placement error is below 0.001 metres and nodes are released.
const METRES_PER_UNIT := 0.0142875
var failures := 0
var target := ""
var camera: Camera3D

func _initialize() -> void:
    _watchdog()
    super._initialize()

func _watchdog() -> void:
    await create_timer(240.0).timeout
    printerr("FAIL hst_23: scene check watchdog")
    quit(1)

func _configure_player() -> void:
    player.set("capture_mouse", false)
    player.set("auto_step", false)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL hst_23: ", message)

func _visual(identity: Dictionary) -> Dictionary:
    for visual: Dictionary in player.call("npc_visuals"):
        if visual.file == identity.file and visual.object == identity.object:
            return visual
    return {}

func _ready_npcs() -> void:
    var deadline := Time.get_ticks_msec() + 90_000
    while Time.get_ticks_msec() < deadline:
        await process_frame
        var counts: Dictionary = player.call("npc_visual_counts")
        if counts.expected > 0 and counts.pending == 0 and counts.drawn + counts.failed == counts.expected:
            _check(counts.failed == 0 and counts.expected == counts.drawn, "every enabled resident NPC draws: " + str(counts) + str(player.call("npc_visuals")))
            return
    _check(false, "NPC asset loading did not finish")

func _capture(name: String) -> Image:
    await process_frame
    await process_frame
    await RenderingServer.frame_post_draw
    _check(root.get_camera_3d() == camera, "capture uses the NPC camera")
    var image := root.get_texture().get_image()
    _check(image.save_png(target.path_join(name + ".png")) == OK, "capture saved")
    return image

func _target_pixels(visible: Image, hidden: Image, center: Vector2) -> int:
    var count := 0
    var rect := Rect2i(Vector2i(center) - Vector2i(55, 110), Vector2i(110, 160))
    rect = rect.intersection(Rect2i(Vector2i.ZERO, visible.get_size()))
    for y in range(rect.position.y, rect.end.y):
        for x in range(rect.position.x, rect.end.x):
            var a := visible.get_pixel(x, y)
            var b := hidden.get_pixel(x, y)
            if maxf(absf(a.r - b.r), maxf(absf(a.g - b.g), absf(a.b - b.b))) > 0.05:
                count += 1
    return count

func _capture_actor(identity: Dictionary, name: String, outside: bool) -> void:
    await _ready_npcs()
    var visual := _visual(identity)
    _check(not visual.is_empty(), name + " is resident")
    if visual.is_empty(): return
    _check(visual.shapes >= 3 and visual.bones > 0 and visual.head_parts > 0 and visual.armor_parts > 0, name + " is a complete skinned character")
    var center: Vector3 = visual.position + Vector3.UP * 0.85
    if outside:
        var feet: Vector3 = player.call("position")
        camera.position = Vector3(feet.x, feet.z + 100.0, -feet.y) * METRES_PER_UNIT
    else:
        var space := player.get_world_3d().direct_space_state
        var clearance := 0.0
        for angle in range(12):
            var direction := Vector3(sin(angle * TAU / 12.0), 0.15, cos(angle * TAU / 12.0)).normalized()
            var candidate := center + direction * 2.0
            var hit := space.intersect_ray(PhysicsRayQueryParameters3D.create(center, candidate))
            var distance := 2.0 if hit.is_empty() else center.distance_to(hit.position) - 0.15
            if distance > clearance:
                clearance = distance
                camera.position = center + direction * distance
        _check(clearance > 1.0, "Hulda camera has a clear view")
    camera.look_at(center)
    camera.current = true
    for node: Node in player.get_children():
        if node.get_class() == "SkinnedActorNode": node.set("playing", false)
    var visible := await _capture(name + "_visible")
    player.call("set_npc_visuals_hidden", true)
    var hidden := await _capture(name + "_hidden")
    var pixels := _target_pixels(visible, hidden, camera.unproject_position(center))
    _check(pixels >= 100, name + " has visible character pixels: " + str(pixels))
    player.call("set_npc_visuals_hidden", false)
    print("HST-23 capture ", name, " target_pixels=", pixels, " shapes=", visual.shapes, " bones=", visual.bones, " camera=", camera.position, " actor=", visual.position)

func _start() -> void:
    target = OS.get_environment("CARGO_TARGET_DIR")
    var manifest: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(target.path_join("hst_23_manifest.json")))
    await super._start()
    if not loaded: return
    system_overlay.queue_free()
    await process_frame
    camera = Camera3D.new()
    root.add_child(camera)
    camera.fov = 65.0
    await _capture_actor(manifest.hulda, "hulda", false)
    var original := _visual(manifest.hulda)
    if original.is_empty():
        quit(1)
        return
    _check(player.call("set_reference_enabled", manifest.hulda.file, manifest.hulda.object, false), "runtime Disable succeeds")
    await process_frame
    await process_frame
    _check(_visual(manifest.hulda).is_empty() and not is_instance_id_valid(original.node_id), "Disable releases actor RIDs and node")
    _check(player.call("set_reference_enabled", manifest.hulda.file, manifest.hulda.object, true), "runtime Enable succeeds")
    await _ready_npcs()
    var restored := _visual(manifest.hulda)
    _check(not restored.is_empty() and restored.position.is_equal_approx(original.position), "Enable rebuilds at retained runtime position")
    var identity: int = restored.get("node_id", 0)
    streamer.call("clear")
    await process_frame
    await process_frame
    _check(player.call("npc_visuals").is_empty() and not is_instance_id_valid(identity), "unloading rendered cell releases all NPC nodes")
    _check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0, "inn reloads")
    await _ready_npcs()
    restored = _visual(manifest.hulda)
    _check(not restored.is_empty() and restored.position.is_equal_approx(original.position), "reloaded actor retains World position")
    var moved := false
    var initial := {}
    for visual: Dictionary in player.call("npc_visuals"):
        initial[visual.object] = visual.position
    for frame in 600:
        player.call("advance", 1.0 / 60.0)
        await process_frame
        for visual: Dictionary in player.call("npc_visuals"):
            var pose: Dictionary = player.call("gameplay_reference_pose", visual.file, visual.object)
            var runtime: Vector3 = pose.position
            var expected := Vector3(runtime.x, runtime.z, -runtime.y) * METRES_PER_UNIT
            _check(expected.distance_to(visual.position) < 0.001, "NPC mesh follows AI runtime pose")
            if initial.has(visual.object):
                moved = moved or visual.position.distance_to(initial[visual.object]) > 0.01
    _check(moved, "at least one inn NPC moves under its AI package")
    print("HST-23 inn AI movement=", moved)
    _check(player.call("activate_reference", manifest.exit.file, manifest.exit.object), "inn exit activates")
    player.call("advance", 0.0)
    await process_frame
    _check(player.call("gameplay_cell") == manifest.exterior_cell, "stands in Whiterun outside the inn door")
    await _capture_actor(manifest.guard, "whiterun_guard", true)
    _check(_visual(manifest.hulda).is_empty(), "Hulda is removed after leaving the inn")
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " hst_23_characters")
    quit(0 if failures == 0 else 1)
