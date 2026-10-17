# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Fixed before baseline: at least 100 water pixels and fewer than half white.
var failures := 0

func _initialize() -> void:
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse", false)
    player.set("auto_step", false)

func _check(ok: bool, label: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 water: ", label)

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
    for overlay in root.find_children("*", "CanvasLayer", true, false):
        overlay.hide()
    player.set_process(false)
    player.set_physics_process(false)
    var camera := root.get_camera_3d()
    var target := Vector3(354.49, -46.635, 90.5)
    camera.global_position = target + Vector3(0, 3, 7)
    camera.look_at(target)
    var water_rids: Array[RID] = []
    var materials: Array[Material] = []
    var water_materials := 0
    for surface: Dictionary in streamer.call("nif_surfaces"):
        var models := str(surface.models).to_lower()
        if not models.contains("wrstairswater01river01"):
            continue
        var material: Material = surface.material
        water_rids.append(surface.rid)
        materials.append(material)
        if material is ShaderMaterial and material.get_shader_parameter("shallow_color") != null:
            water_materials += 1
        print("R4 WATER source=", surface.source, " models=", surface.models, " refs=", surface.references, " class=", material.get_class(), " flags=", [surface.flags1, surface.flags2], " bounds=", surface.bounds)
        if material is StandardMaterial3D:
            print("R4 WATER STANDARD vertex_color=", material.vertex_color_use_as_albedo, " transparency=", material.transparency, " albedo=", material.albedo_color)
        elif material is ShaderMaterial:
            for key in ["mesh_uv", "base_alpha", "shallow_color", "deep_color", "reflection_color", "has_noise", "has_flow", "sun_power", "sun_magnitude", "sparkle_power", "sparkle_magnitude"]:
                print("R4 WATER UNIFORM ", key, "=", material.get_shader_parameter(key))
    _check(not water_rids.is_empty(), "stream meshes present")
    _check(water_materials == 1, "animated stream uses one WATR material")
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    var normal := root.get_texture().get_image()
    normal.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("water_normal.png"))
    for rid in water_rids:
        RenderingServer.instance_set_visible(rid, false)
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    var hidden := root.get_texture().get_image()
    hidden.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("water_hidden.png"))
    var pixels := 0
    var white := 0
    for y in normal.get_height():
        for x in normal.get_width():
            var a := normal.get_pixel(x, y)
            var b := hidden.get_pixel(x, y)
            if absf(a.r-b.r) + absf(a.g-b.g) + absf(a.b-b.b) > 0.05:
                pixels += 1
                white += int(minf(a.r, minf(a.g, a.b)) > 0.3 and maxf(a.r, maxf(a.g, a.b)) - minf(a.r, minf(a.g, a.b)) < 0.12)
    _check(pixels >= 100, "water contributes at least 100 pixels")
    _check(white < pixels * 0.5, "stream is not flat white")
    print("R4 WATER PIXELS total=", pixels, " white=", white)
    for rid in water_rids:
        RenderingServer.instance_set_visible(rid, true)
    for i in materials.size():
        if materials[i] is ShaderMaterial and materials[i].get_shader_parameter("shallow_color") != null:
            RenderingServer.instance_set_visible(water_rids[i], false)
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    root.get_texture().get_image().save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("water_no_plane.png"))
    for i in materials.size():
        RenderingServer.instance_set_visible(water_rids[i], materials[i] is ShaderMaterial and materials[i].get_shader_parameter("shallow_color") != null)
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    root.get_texture().get_image().save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("water_no_foam.png"))
    for rid in water_rids:
        RenderingServer.instance_set_visible(rid, true)
    for material in materials:
        if material is ShaderMaterial and material.get_shader_parameter("shallow_color") != null:
            material.set_shader_parameter("sun_magnitude", 0.0)
            material.set_shader_parameter("sparkle_magnitude", 0.0)
        if material is StandardMaterial3D:
            material.vertex_color_use_as_albedo = true
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    root.get_texture().get_image().save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("water_controls.png"))
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 water probe")
    quit(0 if failures == 0 else 1)

func _process(_delta: float) -> bool:
    return false
