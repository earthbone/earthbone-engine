extends SceneTree

## The installed authored poses supply independent vertex and collision probes.
## Fixed limits: 0.0002 m of vertex error, 0.0002 m of hinge error, zero root drift.

var failures := 0

func _initialize() -> void:
    call_deferred("_run")

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL door_sequences: ", message)

func _vector(value: Array) -> Vector3:
    return Vector3(value[0], value[1], value[2])

func _settle() -> void:
    await physics_frame
    await physics_frame
    await process_frame

func _pose(player: Node3D, streamer: Node3D, entry: Dictionary, sample: Dictionary, root_pose: Transform3D) -> void:
    var state: Dictionary = streamer.call("door_state", int(entry.file), int(entry.object))
    _check(state.transform.is_equal_approx(root_pose), "reference control transform stays fixed")
    _check(state.placement_transform.is_equal_approx(root_pose), "reference placement stays fixed")
    _check(state.get("sequence", "") == sample.sequence, "authored sequence is playing")
    var actual: PackedVector3Array = state.world_vertices
    _check(actual.size() == sample.vertices.size(), "all leaf vertices stay resident")
    var error := 0.0
    for i in mini(actual.size(), sample.vertices.size()):
        error = maxf(error, actual[i].distance_to(_vector(sample.vertices[i])))
    _check(error < 0.0002, "all vertices match the sampled child pose: " + str(error))
    var nodes: Array = state.get("model_nodes", [])
    for hinge: Dictionary in sample.hinges:
        _check(nodes.size() > int(hinge.node), "animated collision node stays bound")
        if nodes.size() > int(hinge.node):
            var transform: Transform3D = nodes[int(hinge.node)]
            _check((root_pose * transform.origin).distance_to(_vector(hinge.position)) < 0.0002, "hinge-side node stays in place")
    for ray: Array in sample.rays:
        var hit: Dictionary = player.call("reference_collision_ray", int(entry.file), int(entry.object), _vector(ray[0]), _vector(ray[1]))
        _check(hit.get("movement", false), "movement collision follows the sampled leaf")
        _check(hit.get("query", false), "host collision follows the sampled leaf")
        _check(hit.get("streamer", false), "streamer collision follows the sampled leaf independently")
    print("door_sequences ", entry.cell, ":", entry.object, " ", sample.sequence, " ", sample.time, " vertex_error=", error)

func _run() -> void:
    var target := OS.get_environment("CARGO_TARGET_DIR")
    if target.is_empty():
        target = ProjectSettings.globalize_path("res://../target")
    var entries = JSON.parse_string(FileAccess.get_file_as_string(target.path_join("door_sequences.json")))
    if not entries is Array:
        printerr("FAIL door_sequences: installed manifest missing")
        quit(1)
        return
    var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
    get_root().add_child(streamer)
    streamer.set_process(false)
    _check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "installed data loads")
    var player: Node3D = ClassDB.instantiate("PlayerNode")
    player.set("capture_mouse", false)
    player.set("auto_step", false)
    get_root().add_child(player)
    for entry: Dictionary in entries:
        _check(player.call("use_streamer_data", streamer), "shared data source")
        _check(streamer.call("stream_cell", entry.cell) > 0, "door cell attaches")
        var position := _vector(entry.position)
        _check(player.call("enter_cell", entry.cell, position.x, position.y, position.z + 1024.0) > 0, "movement collision attaches")
        _check(player.call("start_gameplay"), "gameplay starts")
        _check(streamer.call("stream_cell_collision", entry.cell) > 0, "streamer collision attaches")
        player.call("advance", 0.0)
        var initial: Dictionary = streamer.call("door_state", int(entry.file), int(entry.object))
        var root_pose: Transform3D = initial.placement_transform
        var identity: int = initial.node
        var children := streamer.get_child_count()
        for cycle in 2:
            _check(player.call("set_reference_open", int(entry.file), int(entry.object), false), "World closes door")
            player.call("advance", 0.0)
            streamer.call("advance_doors", 10.0)
            await _settle()
            await _pose(player, streamer, entry, entry.samples[0] if cycle == 0 else entry.samples[4], root_pose)
            _check(player.call("set_reference_open", int(entry.file), int(entry.object), true), "World opens door")
            player.call("advance", 0.0)
            streamer.call("advance_doors", entry.samples[1].time)
            player.call("advance", 0.0)
            await _settle()
            await _pose(player, streamer, entry, entry.samples[1], root_pose)
            streamer.call("advance_doors", entry.samples[1].time)
            player.call("advance", 0.0)
            await _settle()
            await _pose(player, streamer, entry, entry.samples[2], root_pose)
            _check(player.call("set_reference_open", int(entry.file), int(entry.object), false), "World starts Close")
            player.call("advance", 0.0)
            streamer.call("advance_doors", entry.samples[3].time)
            player.call("advance", 0.0)
            await _settle()
            await _pose(player, streamer, entry, entry.samples[3], root_pose)
            streamer.call("advance_doors", entry.samples[3].time)
            player.call("advance", 0.0)
            await _settle()
            await _pose(player, streamer, entry, entry.samples[4], root_pose)
            var final: Dictionary = streamer.call("door_state", int(entry.file), int(entry.object))
            _check(final.node == identity and final.instances == initial.instances and streamer.get_child_count() == children, "node and render identities stay fixed")
        if entry.teleport:
            _check(player.call("activate_reference", int(entry.file), int(entry.object)), "load activation starts Open")
            _check(player.call("gameplay_cell") == entry.cell, "load door stays in source cell during opening")
            player.call("advance", entry.samples[1].time)
            _check(player.call("gameplay_cell") == entry.cell, "load door does not teleport at the halfway pose")
            await _settle()
            await _pose(player, streamer, entry, entry.samples[1], root_pose)
            player.call("advance", entry.samples[1].time + 0.001)
            _check(player.call("gameplay_cell") != entry.cell, "load door teleports after Open")
        streamer.call("clear")
        _check(not is_instance_id_valid(identity), "cell detach frees resident door")
    player.free()
    streamer.free()
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " door_sequences")
    quit(0 if failures == 0 else 1)
