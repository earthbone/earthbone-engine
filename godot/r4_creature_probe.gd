# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Fixed before baseline: p99 edge stretch <= 3 and max <= 8 for edges > 1 mm.
var failures := 0
var metadata: Array = []

func _initialize() -> void:
    var path := OS.get_environment("SOE_CREATURE_METADATA")
    if path.is_empty():
        path = ProjectSettings.globalize_path("res://").trim_suffix("/").get_base_dir().path_join("target/r4-creature-metadata.json")
    metadata = JSON.parse_string(FileAccess.get_file_as_string(path))
    for actor: Dictionary in metadata:
        if actor.race == "GiantRace":
            startup_cell = actor.entry_cell
            startup_radius = 2
            startup_position = Vector3(actor.position[0], actor.position[1] - 700.0, actor.position[2] + 200.0)
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse", false)

func _check(ok: bool, label: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 creature: ", label)

func _wait_actor(actor: Dictionary) -> Node3D:
    for frame in 2400:
        for visual: Dictionary in player.call("npc_visuals"):
            if visual.file == actor.file and visual.object == actor.object and visual.node_id != 0:
                var node := instance_from_id(visual.node_id) as Node3D
                if not node.call("play_path_observation").poses.is_empty():
                    return node
        await process_frame
    return null

func _sample(node: Node3D, actor: Dictionary, phase: int) -> void:
    var observation: Dictionary = node.call("play_path_observation")
    print("R4 CREATURE live=", actor.race, " phase=", phase, " skeleton=", node.call("skeleton_path"), " clips=", node.call("active_clips"), " shapes=", observation.shapes.size(), " poses=", observation.poses.size())
    for variant: Dictionary in actor.variants:
        if variant.surfaces.size() != observation.shapes.size():
            continue
        var matches := true
        for index in observation.shapes.size():
            var shape: Dictionary = observation.shapes[index]
            var arrays: Array = shape.mesh.surface_get_arrays(0)
            var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
            if shape.name != variant.surfaces[index].name or vertices.size() != int(variant.surfaces[index].vertices):
                matches = false
        if matches:
            actor.surfaces = variant.surfaces
            print("R4 CREATURE variant=", variant.path)
            break
    _check(observation.shapes.size() == actor.surfaces.size(), actor.race + " source shape count")
    if observation.shapes.size() != actor.surfaces.size():
        return
    var ratios: Array[float] = []
    var posed_bounds := AABB()
    var first := true
    var offset := 0
    for shape_index in observation.shapes.size():
        var shape: Dictionary = observation.shapes[shape_index]
        var source: Dictionary = actor.surfaces[shape_index]
        _check(shape.name == source.name, actor.race + " source shape name")
        var arrays: Array = shape.mesh.surface_get_arrays(0)
        var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
        var bones: PackedInt32Array = arrays[Mesh.ARRAY_BONES]
        var weights: PackedFloat32Array = arrays[Mesh.ARRAY_WEIGHTS]
        var indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
        var posed := PackedVector3Array()
        posed.resize(vertices.size())
        for vertex in vertices.size():
            var point := Vector3.ZERO
            for influence in 4:
                var index := vertex * 4 + influence
                if weights[index] > 0.0:
                    point += (observation.poses[offset + bones[index]] * vertices[vertex]) * weights[index]
            posed[vertex] = point
            var world: Vector3 = observation.placement * point
            posed_bounds = AABB(world, Vector3.ZERO) if first else posed_bounds.expand(world)
            first = false
        offset += int(source.bones)
        for triangle in range(0, indices.size(), 3):
            for edge in 3:
                var a := indices[triangle + edge]
                var b := indices[triangle + (edge + 1) % 3]
                var length := vertices[a].distance_to(vertices[b])
                if length > 0.001:
                    ratios.append(posed[a].distance_to(posed[b]) / length)
    _check(offset == observation.poses.size(), actor.race + " skin upload count")
    ratios.sort()
    _check(not ratios.is_empty(), actor.race + " measured edges")
    var p99 := ratios[mini(ratios.size()-1, int(ratios.size()*0.99))]
    var maximum: float = ratios.back()
    print("R4 CREATURE stretch race=", actor.race, " phase=", phase, " edges=", ratios.size(), " p99=", p99, " max=", maximum, " bounds=", posed_bounds)
    _check(p99 <= 3.0 and maximum <= 8.0, actor.race + " keeps authored edge lengths")
    var camera := root.get_camera_3d()
    var center := posed_bounds.get_center()
    camera.global_position = center + observation.placement.basis.orthonormalized() * Vector3(0.0, posed_bounds.size.y * 0.15, -posed_bounds.size.length() * 1.3)
    camera.look_at(center)
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    root.get_texture().get_image().save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join(actor.race + "_" + str(phase) + ".png"))

func _run() -> void:
    await super._start()
    if not loaded:
        quit(1)
        return
    for overlay in root.find_children("*", "CanvasLayer", true, false):
        overlay.hide()
    for visual in [player_body, player_arms]:
        visual.set_process(false)
        visual.call("set_first_person_geometry_visible", false)
    var checked := 0
    for actor: Dictionary in metadata:
        if actor.race not in ["GiantRace", "MammothRace"]:
            continue
        if checked > 0:
            player.set_process(true)
            player.set_physics_process(true)
            _check(player.call("queue_player_input", {"action":"move_to", "file":int(actor.file), "object":int(actor.object), "offset":Vector3(0,-700,200), "match_rotation":false}), "creature move")
            _check(player.call("step_player_input"), "creature move applied")
        var node: Node3D = await _wait_actor(actor)
        _check(node != null, actor.race + " resident appears")
        if node == null:
            continue
        player.set_process(false)
        player.set_physics_process(false)
        for visual: Dictionary in player.call("npc_visuals"):
            if visual.node_id != 0:
                var resident := instance_from_id(visual.node_id)
                resident.set_process(false)
        await _sample(node, actor, 0)
        node.call("advance", 0.5)
        await _sample(node, actor, 1)
        checked += 1
    _check(checked >= 2, "giant and mammoth rendered through the free walk")
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 creature probe")
    quit(0 if failures == 0 else 1)

func _process(_delta: float) -> bool:
    return false
