# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Lists every resident NIF particle system in the start cell and captures
## the player view at each of the first few, with births counted over time.
var failures := 0

func _initialize() -> void:
    var cell := OS.get_environment("SOE_SURVEY_CELL")
    if cell != "":
        startup_cell = cell
    if OS.get_environment("SOE_SURVEY_EXTERIOR") == "1":
        startup_radius = 1
        startup_position = Vector3(0.0, 80.0, 0.0)
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse",false)

func _wait_seconds(seconds: float) -> void:
    var started := Time.get_ticks_usec()
    while Time.get_ticks_usec()-started < seconds*1000000.0:
        await process_frame

func _run() -> void:
    await super._start()
    if not loaded:
        quit(1)
        return
    if OS.get_environment("SOE_SURVEY_EXIT") == "1":
        for door: Dictionary in player.call("gameplay_doors"):
            if not door.locked:
                player.call("activate_reference", door.file, door.object)
                break
        for frame in 1800:
            await process_frame
            if frame > 90 and not player.call("is_load_door_pending") and not player.call("is_location_preparing"):
                break
        for frame in 300:
            await process_frame
    var origin := root.get_camera_3d().global_position
    player.set_process(false)
    player.set_physics_process(false)
    for overlay in root.find_children("*","CanvasLayer",true,false):
        overlay.hide()
    await _wait_seconds(2.0)
    var out := OS.get_environment("SOE_CHARACTER_OUTPUT")
    var camera := root.get_camera_3d()
    var rows: Array = []
    for row: Dictionary in streamer.call("nif_surfaces"):
        if row.get("particle",false):
            rows.append(row)
    rows.sort_custom(func(a, b): return a.bounds.get_center().distance_to(origin) < b.bounds.get_center().distance_to(origin))
    print("SURVEY particle systems: ", rows.size(), " origin=", origin)
    for row: Dictionary in streamer.call("nif_surfaces"):
        if not row.get("particle",false) and row.references.has("0:05D691"):
            print("SURVEY other src=", row.source, " keys=", row.keys(), " bounds=", row.bounds)
    if OS.get_environment("SOE_SURVEY_HIDE") == "particles":
        for row in rows:
            RenderingServer.instance_set_visible(row.rid, false)
    var index := 0
    for row in rows:
        var b: AABB = row.bounds
        print("SURVEY ", index, " name=", row.get("particle_system"), " refs=", row.references, " src=", row.source, " center=", b.get_center(), " size=", b.size, " rate=", row.get("nominal_rate"), " emitted=", row.get("emitted"), " cap=", row.get("capacity"))
        index += 1
    var limit := mini(rows.size(), int(OS.get_environment("SOE_SURVEY_SHOTS") if OS.get_environment("SOE_SURVEY_SHOTS") != "" else "6"))
    for i in limit:
        var b: AABB = rows[i].bounds
        var c := b.get_center()
        camera.global_position = c + Vector3(0.0, 1.0, 3.0)
        camera.look_at(c)
        await _wait_seconds(1.5)
        for frame in 3:
            await process_frame
        await RenderingServer.frame_post_draw
        var image := root.get_texture().get_image()
        print("SURVEY shot ", i, " ", image.get_size(), " save=", image.save_png(out.path_join("particle_%02d.png" % i)))
    print("SURVEY done")
    quit(0)

func _process(_delta: float) -> bool:
    return false
