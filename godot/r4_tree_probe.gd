# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Fixed before baseline: no tree component of eight pixels may have 16 clear sky pixels below it.
var failures := 0

func _initialize() -> void:
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse", false)
    player.set("auto_step", false)

func _check(ok: bool, label: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 tree: ", label)

func _settle() -> void:
    for frame in 1800:
        player.call("advance", 0.0)
        await process_frame
        if frame > 90 and not player.call("is_load_door_pending") and not player.call("is_location_preparing"):
            break
    _check(player.call("gameplay_location_ready"), "location ready")

func _image() -> Image:
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    return root.get_texture().get_image()

func _detached(normal: Image, control: Image, view: int) -> int:
    var width := normal.get_width()
    var height := normal.get_height()
    var mask := PackedByteArray()
    mask.resize(width * height)
    for y in height:
        for x in width:
            var a := normal.get_pixel(x, y)
            var b := control.get_pixel(x, y)
            mask[y * width + x] = int(absf(a.r-b.r) + absf(a.g-b.g) + absf(a.b-b.b) > 0.1)
    var sky := control.get_pixel(width / 2, 0)
    var detached := 0
    for start in mask.size():
        if mask[start] == 0:
            continue
        var queue: Array[int] = [start]
        mask[start] = 0
        var at := 0
        var bottom := start / width
        var min_x := start % width
        var max_x := min_x
        while at < queue.size():
            var index := queue[at]
            at += 1
            var x := index % width
            var y := index / width
            bottom = maxi(bottom, y)
            min_x = mini(min_x, x)
            max_x = maxi(max_x, x)
            for dy in range(-1, 2):
                for dx in range(-1, 2):
                    var nx := x + dx
                    var ny := y + dy
                    if nx < 0 or nx >= width or ny < 0 or ny >= height:
                        continue
                    var next := ny * width + nx
                    if mask[next] != 0:
                        mask[next] = 0
                        queue.append(next)
        if queue.size() < 8 or bottom > height * 0.7 or bottom + 16 >= height:
            continue
        var clear := true
        for y in range(bottom + 1, bottom + 17):
            for x in range(min_x, max_x + 1):
                var c := control.get_pixel(x, y)
                if absf(c.r-sky.r) + absf(c.g-sky.g) + absf(c.b-sky.b) > 0.08:
                    clear = false
        if clear:
            detached += 1
            print("R4 TREE detached view=", view, " pixels=", queue.size(), " x=", [min_x, max_x], " bottom=", bottom)
    return detached

func _run() -> void:
    await super._start()
    if not loaded:
        quit(1)
        return
    for door: Dictionary in player.call("gameplay_doors"):
        if not door.locked:
            _check(player.call("activate_reference", door.file, door.object), "inn exit")
            break
    await _settle()
    var gate: Dictionary = player.call("detection_gate_reference")
    _check(not gate.is_empty(), "city gate")
    if gate.is_empty():
        quit(1)
        return
    _check(player.call("activate_reference", gate.file, gate.object), "city exit")
    await _settle()
    _check(player.call("queue_player_input", {"action": "move_to", "file": gate.far_file, "object": gate.far_object, "offset": Vector3(-4096, -4096, 500), "match_rotation": false}), "plains move")
    _check(player.call("step_player_input"), "plains move applied")
    await _settle()
    for overlay in root.find_children("*", "CanvasLayer", true, false):
        overlay.hide()
    player.set_process(false)
    player.set_physics_process(false)
    var camera := root.get_camera_3d()
    var environment: Environment = root.world_3d.environment.duplicate()
    environment.background_mode = Environment.BG_COLOR
    environment.background_color = Color(0.15, 0.25, 0.45)
    environment.fog_enabled = false
    environment.volumetric_fog_enabled = false
    camera.environment = environment
    var origin_rids: Array[RID] = []
    var tree_rids: Array[RID] = []
    for surface: Dictionary in streamer.call("nif_surfaces"):
        if str(surface.models).to_lower().contains("tree"):
            tree_rids.append(surface.rid)
            print("R4 TREE SURFACE models=", surface.models, " refs=", surface.references, " bounds=", surface.bounds)
            if str(surface.models).to_lower().contains("treepineforest") and surface.bounds.get_center().length() < 30.0:
                origin_rids.append(surface.rid)
    _check(origin_rids.is_empty(), "no unplaced pine surfaces near world origin")
    print("R4 TREE origin surfaces=", origin_rids.size())
    var distant: PackedStringArray = streamer.call("distant_references")
    print("R4 TREE DISTANT ", distant)
    print("R4 TREE camera=", camera.global_position, " player=", player.call("position"), " stats=", streamer.call("tree_lod_stats"))
    var detached := 0
    for view in 6:
        var angle := float(view) * TAU / 6.0
        camera.look_at(camera.global_position + Vector3(sin(angle), 0.08, cos(angle)))
        streamer.call("set_tree_lod_visible", true)
        streamer.call("set_object_lod_visible", true)
        var normal: Image = await _image()
        normal.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("tree_%d_normal.png" % view))
        if view == 4:
            var sky := normal.get_pixel(320, 0)
            var floating_pixels := 0
            for y in range(146, 169):
                for x in range(343, 357):
                    var c := normal.get_pixel(x, y)
                    floating_pixels += int(absf(c.r-sky.r) + absf(c.g-sky.g) + absf(c.b-sky.b) > 0.08)
            print("R4 TREE floating region pixels=", floating_pixels)
            _check(floating_pixels <= 3, "floating pine region clears to sky")
        streamer.call("set_tree_lod_visible", false)
        var control: Image = await _image()
        control.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("tree_%d_no_billboards.png" % view))
        detached += _detached(normal, control, view)
        streamer.call("set_object_lod_visible", false)
        var no_objects: Image = await _image()
        no_objects.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("tree_%d_no_lod.png" % view))
        var hidden: Array[RID] = []
        for surface: Dictionary in streamer.call("play_path_surfaces"):
            if surface.kind != "object" or not tree_rids.has(surface.rid):
                continue
            var full := false
            for reference in surface.references:
                full = full or distant.has(reference)
            if full:
                hidden.append(surface.rid)
                RenderingServer.instance_set_visible(surface.rid, false)
        var no_full: Image = await _image()
        no_full.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("tree_%d_no_full.png" % view))
        detached += _detached(no_objects, no_full, view)
        for rid in hidden:
            RenderingServer.instance_set_visible(rid, true)
    _check(detached == 0, "no detached sky trees")
    print("R4 TREE detached total=", detached)
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 tree probe")
    quit(0 if failures == 0 else 1)

func _process(_delta: float) -> bool:
    return false
