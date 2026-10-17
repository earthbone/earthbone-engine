# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Freeze the walkthrough at the pond and separate LOD from loaded LAND.
var failures := 0

func _initialize() -> void:
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse", false)
    player.set("auto_step", false)

func _check(ok: bool, label: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 terrain: ", label)

func _settle() -> void:
    for frame in 1800:
        player.call("advance", 0.0)
        await process_frame
        if frame > 90 and not player.call("is_load_door_pending") and not player.call("is_location_preparing"):
            break
    _check(player.call("gameplay_location_ready"), "location ready")

func _run() -> void:
    await super._start()
    if not loaded:
        quit(1)
        return
    var exited := false
    for door: Dictionary in player.call("gameplay_doors"):
        if not door.locked:
            exited = player.call("activate_reference", door.file, door.object)
            break
    _check(exited, "inn exit")
    await _settle()
    _check(player.call("queue_player_input", {"action": "move_to", "file": 0, "object": 0x0E6C71, "offset": Vector3(-200, 0, 300), "match_rotation": true}), "pond move")
    _check(player.call("step_player_input"), "pond move applied")
    await _settle()
    print("R4 terrain position=", player.call("position"))
    for overlay in root.find_children("*", "CanvasLayer", true, false):
        overlay.hide()
    var camera := root.get_camera_3d()
    var target := camera.global_position
    camera.global_position += Vector3(0, 4, 4)
    camera.look_at(target)
    for mesh: ArrayMesh in streamer.call("loaded_terrain_meshes"):
        var material := mesh.surface_get_material(0) as StandardMaterial3D
        var texture := material.albedo_texture
        var passes := 0
        var pass_material: Material = material
        while pass_material != null:
            passes += 1
            pass_material = pass_material.next_pass
        print("R4 LAND texture=", texture.get_size(), " vertices=", mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size(), " material_passes=", passes)
    var land_target := target
    var closest := INF
    var closest_texture := Vector2.ZERO
    for mesh: ArrayMesh in streamer.call("loaded_terrain_meshes"):
        var arrays := mesh.surface_get_arrays(0)
        var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
        var indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
        for triangle in range(0, indices.size(), 3):
            var point := (vertices[indices[triangle]] + vertices[indices[triangle+1]] + vertices[indices[triangle+2]]) / 3.0
            var distance := point.distance_squared_to(target + Vector3(8, -2, 0))
            if distance < closest:
                closest = distance
                land_target = point
                var land_material := mesh.surface_get_material(0) as StandardMaterial3D
                closest_texture = land_material.albedo_texture.get_size()
    _check(closest_texture.x >= 512, "near LAND retains a full detail diffuse instead of a 256 pixel cell bake: " + str(closest_texture))
    print("R4 LAND target=", land_target, " texture=", closest_texture)
    camera.global_position = land_target + Vector3(0, 2, 1)
    camera.look_at(land_target)
    var full_rids: Array[RID] = []
    for surface: Dictionary in streamer.call("play_path_surfaces"):
        if surface.kind == "lod":
            full_rids.append(surface.rid)
    var normal: Image
    for mode in ["normal", "no-lod", "no-land"]:
        streamer.call("set_terrain_lod_visible", mode == "normal")
        streamer.call("set_object_lod_visible", mode == "normal")
        streamer.call("set_tree_lod_visible", mode == "normal")
        for rid in full_rids:
            RenderingServer.instance_set_visible(rid, mode == "normal")
        streamer.call("set_loaded_terrain_visible", mode != "no-land")
        for frame in 3:
            await process_frame
        await RenderingServer.frame_post_draw
        var image := root.get_texture().get_image()
        image.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("terrain_" + mode + ".png"))
        if mode == "normal":
            normal = image
        else:
            var changed := 0
            for y in range(image.get_height()/2, image.get_height()):
                for x in image.get_width():
                    var a := image.get_pixel(x, y)
                    var b := normal.get_pixel(x, y)
                    changed += int(absf(a.r-b.r) + absf(a.g-b.g) + absf(a.b-b.b) > 0.05)
            print("R4 terrain mode=", mode, " changed_lower_pixels=", changed)
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 terrain probe")
    quit(0 if failures == 0 else 1)

func _process(_delta: float) -> bool:
    return false
