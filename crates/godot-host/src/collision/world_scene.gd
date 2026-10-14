extends "res://walkthrough.gd"

const UNITS := 69.99125
const STEP := 1.0 / 60.0
var failures := 0

func _initialize() -> void:
    startup_position = Vector3(-125, -800, 120)
    super._initialize()

func _configure_player() -> void:
    player.set("capture_mouse", false)
    player.set("auto_step", false)
    player.set("exterior_lod_full_res_radius", 2)
    player.set("exterior_lod_radius", 3)

func _process(_delta: float) -> bool:
    return false

func _check(ok: bool, message: String) -> bool:
    if not ok:
        failures += 1
        printerr("WORLD_SCENE FAIL ", message)
    return ok

func _tick(count := 1) -> void:
    for frame in count:
        player.call("advance", STEP)
        if frame % 4 == 3:
            await process_frame
    await process_frame

func _pose(item: Dictionary) -> Dictionary:
    for pose: Dictionary in streamer.call("movable_reference_poses"):
        if pose.file == item.file and pose.object == item.object:
            return pose
    return {}

func _center(body: RID) -> Vector3:
    var transform: Transform3D = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM)
    var total := Vector3.ZERO
    var count := 0
    for i in PhysicsServer3D.body_get_shape_count(body):
        var shape = PhysicsServer3D.shape_get_data(PhysicsServer3D.body_get_shape(body, i))
        var placement := transform * PhysicsServer3D.body_get_shape_transform(body, i)
        if shape is PackedVector3Array:
            for point: Vector3 in shape:
                total += placement * point
                count += 1
    var center := total / float(count) if count > 0 else transform.origin
    return Vector3(center.x, -center.z, center.y) * UNITS

func _start() -> void:
    create_timer(240.0).timeout.connect(func() -> void:
        printerr("WORLD_SCENE FAIL watchdog")
        quit(1))
    _check(OS.get_environment("DISPLAY") == ":99", "private Xvfb display")
    await super._start()
    if not loaded:
        quit(1)
        return
    var exited := false
    for door: Dictionary in player.call("gameplay_doors"):
        if not door.locked and door.get("far_worldspace", "") == "WhiterunWorld":
            exited = player.call("activate_reference", door.file, door.object)
            break
    _check(exited, "authored inn exit")
    player.call("advance", 0.0)
    await _tick(240)
    _check(not player.call("door_fade_active"), "inn exit fade has completed")
    var gate: Dictionary = player.call("detection_gate_reference")
    _check(not gate.is_empty(), "Whiterun gate reference")
    if not gate.is_empty():
        _check(player.call("queue_player_input", {"action": "move_to", "file": gate.file, "object": gate.object, "offset": Vector3(0, 200, 0), "match_rotation": true}), "gate placement queued")
        _check(player.call("step_player_input"), "gate placement applied")
    var deadline := Time.get_ticks_msec() + 90_000
    var counts: Dictionary = {}
    while Time.get_ticks_msec() < deadline:
        await _tick()
        counts = player.call("npc_visual_counts")
        if counts.expected > 0 and counts.pending == 0 and counts.drawn + counts.failed == counts.expected:
            break
    var expected: PackedStringArray = player.call("exterior_actor_reference_ids")
    _check(not expected.is_empty(), "enabled exterior ACHRs are nonempty")
    _check(counts.expected == expected.size() and counts.drawn == expected.size() and counts.failed == 0, "exterior NPC spawn count " + str(counts) + " records=" + str(expected.size()))
    print("WORLD_NPCS ", JSON.stringify({"spawned": counts.drawn, "expected": expected.size()}))
    var grounded: Array = player.call("npc_grounding_status")
    _check(not grounded.is_empty(), "exterior NPC grounding survey")
    for actor: Dictionary in grounded:
        _check(actor.has("ground_z") or actor.has("navmesh_z"), "collision or NAVM under NPC " + str(actor))
        if actor.has("ground_z"):
            _check(actor.below_ground <= 32, "NPC below collision " + str(actor))
    var items: Array[Dictionary] = []
    var stalls: Array[Dictionary] = []
    for reference: Dictionary in player.call("play_path_references"):
        if reference.model.to_lower().contains("wrmarketstand"):
            stalls.append(reference)
    for reference: Dictionary in player.call("play_path_references"):
        if reference.kind not in ["ALCH", "INGR", "MISC", "WEAP", "BOOK"]:
            continue
        for stall: Dictionary in stalls:
            var distance: Vector3 = reference.position - stall.position
            if Vector2(distance.x, distance.y).length() < 220 and distance.z > -100 and distance.z < 180:
                reference.stall = stall
                items.append(reference)
                break
    _check(items.size() > 0, "stall item survey is nonempty")
    var reached := 0
    for item: Dictionary in items:
        var pose := _pose(item)
        if not _check(not pose.is_empty(), "stall body " + item.id):
            continue
        var before: Dictionary = player.call("activation_reference_status", item.file, item.object)
        var stall_pose: Dictionary = player.call("gameplay_reference_pose", item.stall.file, item.stall.object)
        var yaw: float = stall_pose.rotation.z
        var front := Vector3(sin(yaw), cos(yaw), 0)
        var aimed := false
        for side in [1.0, -1.0]:
            var offset: Vector3 = front * side * 115
            var support: Dictionary = player.call("play_path_ground", item.position + offset + Vector3(0, 0, 24))
            if not _check(support.has("height"), "collision under front approach " + item.id):
                continue
            offset.z = support.height - item.position.z + 1
            _check(player.call("queue_player_input", {"action": "move_to", "file": item.file, "object": item.object, "offset": offset, "match_rotation": false}), "front approach queued")
            _check(player.call("step_player_input"), "front approach applied")
            await _tick(2)
            var eye_godot: Vector3 = get_root().get_camera_3d().global_position
            var eye := Vector3(eye_godot.x, -eye_godot.z, eye_godot.y) * UNITS
            var live_pose := _pose(item)
            if not _check(not live_pose.is_empty(), "resident stall body after approach " + item.id):
                continue
            var direction := _center(live_pose.body) - eye
            var pitch := atan2(direction.z, Vector2(direction.x, direction.y).length())
            _check(player.call("queue_player_input", {"action": "turn", "rotation": Vector3(-pitch, 0, atan2(direction.x, direction.y))}), "aim queued")
            _check(player.call("step_player_input"), "aim applied")
            await _tick()
            var hit: Dictionary = player.call("carry_status")
            if hit.get("crosshair_file", -1) == item.file and hit.get("crosshair_object", -1) == item.object:
                aimed = true
                break
        if not _check(aimed, "front activation ray " + item.id):
            continue
        reached += 1
        var prompt: Dictionary = player.call("activation_prompt")
        _check(prompt.get("action", "") in ["Take", "Steal"], "pickup prompt " + item.id)
        var event := InputEventKey.new()
        event.keycode = KEY_E
        event.pressed = true
        Input.parse_input_event(event)
        Input.flush_buffered_events()
        await _tick()
        event.pressed = false
        Input.parse_input_event(event)
        Input.flush_buffered_events()
        await _tick(2)
        var after: Dictionary = player.call("activation_reference_status", item.file, item.object)
        _check(after.count > before.count and not after.enabled, "take item " + item.id)
        if prompt.stealing:
            _check(after.owned_count > before.owned_count, "stolen ownership " + item.id)
    _check(reached == items.size(), "every stall item reached")
    print("WORLD_STALL ", JSON.stringify({"items": items.size(), "ray_hits": reached}))
    print("WORLD_SCENE PASS" if failures == 0 else "WORLD_SCENE FAIL")
    quit(0 if failures == 0 else 1)
