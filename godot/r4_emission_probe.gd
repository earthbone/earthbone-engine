# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Fixed before baseline: three authored emitters, positive births within
## rate limits over five seconds, 20 smoke and spark pixels, then expiry.
## Lower rate bound before the finished renderer run: 80 percent of the
## smaller of nominal births and capacity, allowing the authored quota.
const FIRE_REF := "0:040911"
var failures := 0

func _initialize() -> void:
    startup_cell = "BleakwindBasinExterior01"
    startup_radius = 2
    startup_position = Vector3(1619.9565,50.8893,-5704.0)
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse",false)

func _check(ok: bool, label: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 emission: ",label)

func _capture() -> Image:
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    return root.get_texture().get_image()

func _wait_seconds(seconds: float) -> void:
    var started := Time.get_ticks_usec()
    while Time.get_ticks_usec()-started < seconds*1000000.0:
        await process_frame

func _systems() -> Array[Dictionary]:
    var result: Array[Dictionary] = []
    for row: Dictionary in streamer.call("nif_surfaces"):
        if row.references.has(FIRE_REF) and row.get("particle",false):
            result.append(row)
    return result

func _pixels(image: Image) -> Vector2i:
    var counts := Vector2i.ZERO
    for y in image.get_height():
        for x in image.get_width():
            var c := image.get_pixel(x,y)
            counts.x += int(c.b > c.r+0.05 and c.b > c.g+0.05)
            counts.y += int(c.g > c.r+0.05 and c.g > c.b+0.05)
    return counts

func _run() -> void:
    _check(OS.get_environment("DISPLAY") == ":99","private display")
    await super._start()
    if not loaded:
        quit(1)
        return
    player.set_process(false)
    player.set_physics_process(false)
    for visual in [player_body,player_arms]:
        visual.set_process(false)
        visual.call("set_first_person_geometry_visible",false)
    for actor: Dictionary in player.call("npc_visuals"):
        if actor.node_id != 0:
            var node := instance_from_id(actor.node_id)
            node.set_process(false)
            node.call("set_first_person_geometry_visible",false)
    for overlay in root.find_children("*","CanvasLayer",true,false):
        overlay.hide()
    for light in root.find_children("*","Light3D",true,false):
        light.hide()
    for node in root.find_children("*","Node",true,false):
        var script: Script = node.get_script()
        if script != null and script.resource_path == "res://record_sky.gd":
            node.set_process(false)
    var systems := _systems()
    _check(systems.size() == 3,"three live giant fire emitters, got "+str(systems.size()))
    var camera := root.get_camera_3d()
    var center := Vector3.ZERO
    var found := false
    for surface: Dictionary in streamer.call("nif_surfaces"):
        if surface.references.has(FIRE_REF) and str(surface.source).to_lower().contains("fxfirescrolltile02"):
            center = surface.bounds.get_center()
            found = true
        if not surface.get("particle",false):
            RenderingServer.instance_set_visible(surface.rid,false)
    _check(found,"real giant fire reference is resident")
    var environment := Environment.new()
    environment.background_mode = Environment.BG_COLOR
    environment.background_color = Color.BLACK
    environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
    camera.environment = environment
    var exposure := CameraAttributesPractical.new()
    exposure.auto_exposure_enabled = false
    exposure.exposure_multiplier = 1.0
    camera.attributes = exposure
    camera.cull_mask = 1 << 19
    camera.fov = 50.0
    camera.global_position = center+Vector3(0.0,2.0,6.0)
    camera.look_at(center+Vector3(0.0,1.0,0.0))
    var markers: Array[Material] = []
    var before := {}
    var rates: Array[float] = []
    for system in systems:
        var material: ShaderMaterial = system.material
        var code := material.shader.code
        if not code.contains("unshaded"):
            code = code.replace("render_mode ","render_mode unshaded, ")
        var name := str(system.particle_system)
        var tag := "vec3(0.0,0.0,1.0)" if name.to_lower().contains("smoke") else ("vec3(0.0,1.0,0.0)" if name.to_lower().contains("spark") else "vec3(1.0,0.0,0.0)")
        var expression := RegEx.new()
        expression.compile("ALBEDO\\s*=.*?;")
        code = expression.sub(code,"ALBEDO = "+tag+";",true)
        expression.compile("EMISSION\\s*=.*?;")
        code = expression.sub(code,"EMISSION = vec3(0.0);",true)
        var shader := Shader.new()
        shader.code = code
        var marker: ShaderMaterial = material.duplicate()
        marker.shader = shader
        markers.append(marker)
        RenderingServer.instance_set_layer_mask(system.rid,1 << 19)
        RenderingServer.instance_geometry_set_material_override(system.rid,marker.get_rid())
        before[name] = {"emitted":system.emitted,"elapsed":system.elapsed}
        rates.append(system.nominal_rate)
        print("R4 EMISSION source=",system)
    rates.sort()
    _check(rates.size() == 3 and absf(rates[0]-3.0)<0.001 and absf(rates[1]-24.0)<0.001 and absf(rates[2]-60.0)<0.001,"authored nominal birth rates")
    var started := Time.get_ticks_usec()
    await _wait_seconds(5.0)
    var elapsed := (Time.get_ticks_usec()-started)/1000000.0
    var image := await _capture()
    var visible := _pixels(image)
    image.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("emission_live.png"))
    _check(visible.x >= 20 and visible.y >= 20,"visible smoke and spark pixels")
    for system in _systems():
        var start: Dictionary = before[system.particle_system]
        var births: int = system.emitted-start.emitted
        var seconds: float = system.elapsed-start.elapsed
        var minimum := floorf(0.8*minf(system.nominal_rate*seconds,system.capacity))
        _check(births >= minimum and births > 0 and births <= ceilf(system.nominal_rate*seconds)+1.0,"submitted authored births")
        _check(system.alive <= system.capacity,"authored live quota")
        print("R4 EMISSION births name=",system.particle_system," births=",births," seconds=",seconds," wall_seconds=",elapsed," minimum=",minimum," alive=",system.alive," capacity=",system.capacity)
    streamer.set_process(false)
    await _wait_seconds(4.8)
    var expired: Image = await _capture()
    var remaining := _pixels(expired)
    expired.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("emission_expired.png"))
    _check(remaining.x <= 3 and remaining.y <= 3,"particles expire after source maximum lifetime")
    print("R4 EMISSION pixels live=",visible," expired=",remaining," systems=",systems.size())
    streamer.call("clear")
    _check(_systems().is_empty(),"cell clear releases all source particle resources")
    print("RESULT: ","PASS" if failures == 0 else "FAIL"," R4 streamed emission")
    quit(0 if failures == 0 else 1)

func _process(_delta: float) -> bool:
    return false
