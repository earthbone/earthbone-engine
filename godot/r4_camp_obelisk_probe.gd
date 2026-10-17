# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## The placed slab must use its MODS texture and contribute at least 100 pixels.
var failures := 0

func _initialize() -> void:
    startup_cell = "BleakwindBasinExterior01"
    startup_radius = 2
    startup_position = Vector3(3607.603, -59.113, -5733.816)
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse", false)

func _check(ok: bool, label: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 obelisk: ", label)

func _run() -> void:
    await super._start()
    if not loaded:
        quit(1)
        return
    for frame in 120:
        await process_frame
    player.set_process(false)
    player.set_physics_process(false)
    for visual in [player_body, player_arms]:
        visual.set_process(false)
        visual.call("set_first_person_geometry_visible", false)
    for overlay in root.find_children("*", "CanvasLayer", true, false):
        overlay.hide()
    var camera := root.get_camera_3d()
    var center := Vector3(3607.603, -5933.816, -640.88715) * 0.0142875
    center.y += 1.5
    camera.global_position = center + Vector3(0, 2, 9)
    camera.look_at(center)
    var rids: Array[RID] = []
    var alternate := 0
    for surface: Dictionary in streamer.call("nif_surfaces"):
        if not surface.references.has("0:0C8E8B"):
            continue
        rids.append(surface.rid)
        print("R4 OBELISK LIVE source=", surface.source, " models=", surface.models, " refs=", surface.references, " bounds=", surface.bounds)
        if str(surface.source).to_lower().replace("\\", "/").ends_with("landscape/mountains/mountainslab02.dds"):
            alternate += 1
    _check(not rids.is_empty(), "placed obelisk is resident")
    _check(alternate >= 1, "authored mountain slab TXST is uploaded")
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    var shown := root.get_texture().get_image()
    shown.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("obelisk.png"))
    for rid in rids:
        RenderingServer.instance_set_visible(rid, false)
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    var hidden := root.get_texture().get_image()
    var pixels := 0
    for y in shown.get_height():
        for x in shown.get_width():
            var a := shown.get_pixel(x,y)
            var b := hidden.get_pixel(x,y)
            pixels += int(absf(a.r-b.r)+absf(a.g-b.g)+absf(a.b-b.b) > 0.05)
    _check(pixels >= 100, "obelisk contributes visible pixels")
    print("R4 OBELISK alternate=", alternate, " pixels=", pixels)
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 camp obelisk")
    quit(0 if failures == 0 else 1)

func _process(_delta: float) -> bool:
    return false
