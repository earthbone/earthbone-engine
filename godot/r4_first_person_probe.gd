# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Fixed before baseline: ready arms need 100 visible pixels with shadows off.
var failures := 0

func _initialize() -> void:
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse", false)

func _check(ok: bool, label: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 arms: ", label)

func _steps(count: int) -> void:
    for frame in count:
        await process_frame

func _capture() -> Image:
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    return root.get_texture().get_image()

func _sample(label: String) -> void:
    player.set_process(false)
    player.set_physics_process(false)
    for visual in [player_body, player_arms]:
        visual.set_process(false)
    var observation: Dictionary = player_arms.call("play_path_observation")
    var camera := root.get_camera_3d()
    print("R4 ARMS state=", label, " skeleton=", player_arms.call("skeleton_path"), " behavior=", player_arms.call("behavior_path"), " clips=", player_arms.call("active_clips"), " placement=", observation.placement, " offset=", observation.camera_offset, " camera=", camera.global_transform, " cull=", camera.cull_mask)
    var marker := ShaderMaterial.new()
    var shader := Shader.new()
    shader.code = "shader_type spatial; render_mode unshaded, cull_disabled; void fragment() { ALBEDO = vec3(1.0, 0.0, 1.0); }"
    marker.shader = shader
    var offset := 0
    for shape: Dictionary in observation.shapes:
        var arrays: Array = shape.mesh.surface_get_arrays(0)
        var bones: PackedInt32Array = arrays[Mesh.ARRAY_BONES]
        var weights: PackedFloat32Array = arrays[Mesh.ARRAY_WEIGHTS]
        var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
        var count := 0
        for bone in bones:
            count = maxi(count, bone + 1)
        var low := Vector3(INF, INF, INF)
        var high := -low
        var in_front := 0
        var in_frustum := 0
        for vertex in vertices.size():
            var point := vertices[vertex] if bones.is_empty() else Vector3.ZERO
            if not bones.is_empty():
                for influence in 4:
                    var index := vertex * 4 + influence
                    point += (observation.poses[offset + bones[index]] * vertices[vertex]) * weights[index]
            point = observation.placement * point
            var local := camera.global_transform.affine_inverse() * point
            low = low.min(local)
            high = high.max(local)
            in_front += int(local.z < -camera.near)
            in_frustum += int(camera.is_position_in_frustum(point))
        offset += count
        print("R4 ARMS shape=", shape.name, " visible=", shape.visible, " vertices=", vertices.size(), " front=", in_front, " frustum=", in_frustum, " camera_bounds=", [low, high])
    var normal: Image = await _capture()
    normal.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join(label + "_normal.png"))
    for shape: Dictionary in observation.shapes:
        RenderingServer.instance_set_visible(shape.rid, false)
    var hidden: Image = await _capture()
    hidden.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join(label + "_hidden.png"))
    for shape: Dictionary in observation.shapes:
        RenderingServer.instance_set_visible(shape.rid, true)
        RenderingServer.instance_geometry_set_cast_shadows_setting(shape.rid, RenderingServer.SHADOW_CASTING_SETTING_OFF)
        RenderingServer.instance_geometry_set_material_override(shape.rid, marker.get_rid())
    var mask: Image = await _capture()
    mask.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join(label + "_mask.png"))
    var geometry := 0
    var visible := 0
    for y in normal.get_height():
        for x in normal.get_width():
            var m := mask.get_pixel(x, y)
            if m.r < 0.2 or m.b < 0.2 or m.g > minf(m.r, m.b) * 0.25:
                continue
            geometry += 1
            var a := normal.get_pixel(x, y)
            var b := hidden.get_pixel(x, y)
            visible += int(absf(a.r-b.r) + absf(a.g-b.g) + absf(a.b-b.b) > 0.03)
    for shape: Dictionary in observation.shapes:
        RenderingServer.instance_geometry_set_material_override(shape.rid, RID())
        RenderingServer.instance_geometry_set_cast_shadows_setting(shape.rid, RenderingServer.SHADOW_CASTING_SETTING_ON)
    print("R4 ARMS pixels state=", label, " geometry=", geometry, " visible=", visible)
    _check(visible >= 100, label + " arms contribute at least 100 visible pixels")
    player.set_process(true)
    player.set_physics_process(true)
    for visual in [player_body, player_arms]:
        visual.set_process(true)

func _settle() -> void:
    for frame in 1800:
        await process_frame
        if frame > 90 and not player.call("is_load_door_pending") and not player.call("is_location_preparing"):
            break
    _check(player.call("gameplay_location_ready"), "door location ready")

func _selected_form(table: RefCounted, signature: String, query: String, exact: bool = true) -> Vector2i:
    var rows: PackedStringArray = table.call("search", signature, query, 0, 200)
    for encoded in rows:
        var form: Resource = table.call("selected_form", encoded)
        if not exact or str(form.get("editor_id")).to_lower() == query.to_lower():
            print("R4 ARMS selected=", form.get("editor_id"), " id=", encoded)
            return Vector2i(form.get("file_index"),form.get("object_id"))
    _check(false, "installed form " + query)
    return Vector2i(-1,-1)

func _additional_states() -> void:
    var table: RefCounted = ClassDB.instantiate("SoeFormTable")
    var paths := PackedStringArray()
    for plugin in ["Skyrim.esm","Update.esm","Dawnguard.esm","HearthFires.esm","Dragonborn.esm"]:
        paths.append(OS.get_environment("SKYRIM_DATA").path_join(plugin))
    _check(table.call("open",paths,"Skyrim.esm"), "installed weapon and spell queries")
    player.call("set_facing",0.0,0.0)
    for weapon in [["greatsword","IronGreatsword"],["bow","HuntingBow"],["crossbow","DLC1CrossBow"],["staff","StaffFirebolt"]]:
        var form := _selected_form(table,"WEAP",weapon[1],weapon[0] != "staff")
        if form.x < 0:
            continue
        var equipped: bool = player.call("equip_melee_weapon",form.x,form.y,false)
        _check(equipped, "equip " + weapon[0])
        if not equipped:
            print("R4 ARMS unavailable state=",weapon[0]," setup=non_melee_hand_weapon")
            continue
        await _steps(90)
        await _sample(weapon[0])
    _check(player.call("unequip_player_hand",false),"clear weapon for spell")
    var spell := _selected_form(table,"SPEL","Flames")
    if spell.x >= 0:
        _check(player.call("equip_player_spell",spell.x,spell.y,false),"equip right Flames")
        await _steps(90)
        await _sample("right_spell")
        _check(player.call("equip_player_spell",spell.x,spell.y,true),"equip left Flames")
        await _steps(90)
        await _sample("dual_spell")

func _run() -> void:
    await super._start()
    if not loaded:
        quit(1)
        return
    for overlay in root.find_children("*", "CanvasLayer", true, false):
        overlay.hide()
    for actor: Dictionary in player.call("npc_visuals"):
        if actor.node_id != 0:
            var node := instance_from_id(actor.node_id)
            node.set("playing", false)
            node.set_process(false)
    _check(not player.call("is_third_person"), "first person camera")
    await _steps(60)
    await _sample("sheathed")
    _check(player.call("toggle_weapon_ready"), "ready unarmed hands")
    await _steps(90)
    await _sample("unarmed")
    _check(player.call("equip_melee_weapon", 0, 0x12EB7, false), "equip iron sword")
    await _steps(90)
    await _sample("sword")
    for door: Dictionary in player.call("gameplay_doors"):
        if not door.locked:
            _check(player.call("activate_reference", door.file, door.object), "inn exit")
            break
    await _settle()
    var gate: Dictionary = player.call("detection_gate_reference")
    _check(not gate.is_empty(), "city gate")
    if not gate.is_empty():
        _check(player.call("activate_reference", gate.file, gate.object), "city exit")
        await _settle()
        await _steps(60)
        await _sample("exterior_sword")
        player.call("set_facing", 0.0, -1.0)
        await _steps(10)
        await _sample("exterior_look_down")
        _check(not player.call("toggle_weapon_ready"), "sheathe before keyboard test")
        await _steps(30)
        var key := InputEventKey.new()
        key.physical_keycode = KEY_R
        key.keycode = KEY_R
        key.pressed = true
        Input.parse_input_event(key)
        await _steps(3)
        key.pressed = false
        Input.parse_input_event(key)
        await _steps(90)
        await _sample("keyboard_ready")
    await _additional_states()
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 first person probe")
    quit(0 if failures == 0 else 1)

func _process(_delta: float) -> bool:
    return false
