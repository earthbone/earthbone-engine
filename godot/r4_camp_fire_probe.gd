# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

var failures := 0
var phase_captures := 0

## Before baseline: sampled UV phase must match the authored CPU phase within
## 0.025, with at least 100 pixels and a calibrated linear colour readback.
func _check(ok: bool, label: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 fire: ", label)

func _capture_phase() -> Vector2:
    for frame in 3:
        await process_frame
    await RenderingServer.frame_post_draw
    var image := root.get_texture().get_image()
    image.save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("fire_phase_"+str(phase_captures)+".png"))
    phase_captures += 1
    var total := 0.0
    var pixels := 0
    for y in image.get_height():
        for x in image.get_width():
            var color := image.get_pixel(x,y)
            if color.b > 0.9 and color.g < 0.05:
                total += color.srgb_to_linear().r
                pixels += 1
    _check(pixels >= 100, "live fire phase readback has at least 100 pixels")
    return Vector2(total/maxi(1,pixels), pixels)

func _verify_clock(surfaces: Array[Dictionary]) -> void:
    streamer.set_process(false)
    var selected: Dictionary = {}
    for surface in surfaces:
        if str(surface.source).to_lower().contains("fxfirescrolltile02"):
            selected = surface
            break
    _check(not selected.is_empty(), "giant camp authored scrolling flame is resident")
    if selected.is_empty():
        return
    var material: ShaderMaterial = selected.material
    for surface in surfaces:
        if surface.rid != selected.rid:
            RenderingServer.instance_set_visible(surface.rid, false)
    var original := material.shader.code
    var end := original.find("varying float angle_opacity;")
    var assignment := ""
    for line in original.split("\n"):
        if line.contains("animated_offset.y = effect_track_"):
            assignment = line
    _check(end > 0 and not assignment.is_empty(), "live shader has its authored V controller")
    if end < 0 or assignment.is_empty():
        return
    var prefix := original.substr(0,end)
    var mode_start := prefix.find("render_mode ")
    var mode_end := prefix.find(";",mode_start)
    prefix = prefix.substr(0,mode_start) + "render_mode unshaded, depth_test_disabled, cull_disabled;" + prefix.substr(mode_end+1)
    var shader := Shader.new()
    shader.code = prefix + "\nuniform float phase_override = -1.0;\nvoid fragment() { vec2 animated_offset=uv_offset; " + assignment + " ALBEDO=vec3(phase_override>=0.0 ? phase_override : animated_offset.y,0.0,1.0); }"
    var debug := ShaderMaterial.new()
    debug.shader = shader
    RenderingServer.instance_geometry_set_material_override(selected.rid, debug.get_rid())
    var camera := root.get_camera_3d()
    var environment := Environment.new()
    environment.background_mode = Environment.BG_COLOR
    environment.background_color = Color(0.0,0.2,0.1)
    environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
    camera.environment = environment
    camera.attributes = CameraAttributesPractical.new()
    camera.attributes.auto_exposure_enabled = false
    camera.attributes.exposure_multiplier = 1.0
    var center: Vector3 = selected.bounds.get_center()
    camera.global_position = center + Vector3(0,1,12)
    camera.look_at(center)
    debug.set_shader_parameter("phase_override", 0.5)
    var calibration := await _capture_phase()
    _check(absf(calibration.x - 0.5) <= 0.025, "phase readback calibration")
    debug.set_shader_parameter("phase_override", -1.0)
    for step in 4:
        var offset: Vector2 = material.get_shader_parameter("uv_offset")
        debug.set_shader_parameter("uv_offset", offset)
        var time = material.get_shader_parameter("animation_time")
        debug.set_shader_parameter("animation_time", -1.0 if time == null else time)
        var sampled := await _capture_phase()
        var error := absf(sampled.x-offset.y)
        print("R4 FIRE CLOCK step=", step, " cpu_phase=", offset.y, " gpu_phase=", sampled.x, " error=", error, " pixels=", sampled.y, " clock=", time, " engine_scale=", Engine.time_scale)
        _check(error <= 0.025, "GPU flame uses authored object playback phase")
        streamer.call("step_object_animations", 0.25)
    RenderingServer.instance_geometry_set_material_override(selected.rid, RID())

func _initialize() -> void:
    startup_cell = "BleakwindBasinExterior01"
    startup_radius = 2
    startup_position = Vector3(1619.9565, 50.8893, -5704.0)
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse", false)

func _run() -> void:
    await super._start()
    if not loaded:
        quit(1)
        return
    player.set_process(false)
    player.set_physics_process(false)
    for visual in [player_body, player_arms]:
        visual.set_process(false)
        visual.call("set_first_person_geometry_visible", false)
    for overlay in root.find_children("*", "CanvasLayer", true, false):
        overlay.hide()
    var camera := root.get_camera_3d()
    var surfaces: Array[Dictionary] = []
    var nearest := 1e9
    var center := Vector3.ZERO
    for surface: Dictionary in streamer.call("nif_surfaces"):
        if not str(surface.models).to_lower().contains("fire"):
            continue
        surfaces.append(surface)
        print("R4 FIRE source=", surface.source, " models=", surface.models, " refs=", surface.references, " animated=", surface.animated, " bounds=", surface.bounds)
        var material: Material = surface.material
        if material is ShaderMaterial:
            print("R4 FIRE SHADER ", material.shader.code)
            for key in ["animation_time", "uv_offset", "uv_scale", "uv_rotation"]:
                print("R4 FIRE UNIFORM ", key, "=", material.get_shader_parameter(key))
            var distance: float = surface.bounds.get_center().distance_to(camera.global_position)
            if distance < nearest:
                nearest = distance
                center = surface.bounds.get_center()
    if surfaces.is_empty():
        failures += 1
        printerr("FAIL R4 fire: no live camp fire surfaces")
    camera.global_position = center + Vector3(0, 1, 12)
    camera.look_at(center)
    for phase in 3:
        for frame in 3:
            await process_frame
        await RenderingServer.frame_post_draw
        root.get_texture().get_image().save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("fire_"+str(phase)+".png"))
        print("R4 FIRE PHASE=", phase, " wall_usec=", Time.get_ticks_usec(), " process_delta=", root.get_process_delta_time())
        for surface in surfaces:
            var material: Material = surface.material
            if material is ShaderMaterial:
                print("R4 FIRE SAMPLE source=", surface.source, " uv=", material.get_shader_parameter("uv_offset"), " animation_time=", material.get_shader_parameter("animation_time"))
        streamer.call("step_object_animations", 0.25)
    await _verify_clock(surfaces)
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 camp fire diagnostic")
    quit(0 if failures == 0 else 1)

func _process(_delta: float) -> bool:
    return false
