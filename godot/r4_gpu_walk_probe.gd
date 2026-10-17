# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Fixed before first run: at least 600 positive GPU samples over ten seconds,
## GPU p99 below 16.67 ms and 100 Skyrim units of walking in each location.
const Route := preload("res://prf_31_walk_route.gd")
var failures := 0
var measurements := {}

func _initialize() -> void:
    call_deferred("_run")

func _configure_player() -> void:
    player.set("capture_mouse", false)

func _check(ok: bool, label: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL R4 GPU: ", label)

func _percentiles(values: Array[float]) -> Dictionary:
    values.sort()
    return {"p50": values[int((values.size()-1)*0.5)], "p95": values[int((values.size()-1)*0.95)], "p99": values[int((values.size()-1)*0.99)], "max": values.back()}

func _settle() -> void:
    for frame in 1800:
        await process_frame
        if frame > 90 and not player.call("is_load_door_pending") and not player.call("is_location_preparing"):
            break
    _check(player.call("gameplay_location_ready"), "location ready")

func _measure(label: String) -> void:
    for frame in 120:
        await process_frame
    var viewport := root.get_viewport_rid()
    RenderingServer.viewport_set_measure_render_time(viewport, true)
    for frame in 30:
        await process_frame
    var gpu: Array[float] = []
    var render_cpu: Array[float] = []
    var process_cpu: Array[float] = []
    var wall: Array[float] = []
    var position: Vector3 = player.call("position")
    var route := Route.new(0.0, position)
    var distance := 0.0
    var key := InputEventKey.new()
    key.physical_keycode = KEY_W
    key.keycode = KEY_W
    key.pressed = true
    Input.parse_input_event(key)
    Input.flush_buffered_events()
    var started := Time.get_ticks_usec()
    var previous := started
    while gpu.size() < 600 or Time.get_ticks_usec()-started < 10000000:
        player.call("set_facing", route.yaw, 0.0)
        await process_frame
        var now := Time.get_ticks_usec()
        var elapsed := (now-previous)/1000000.0
        previous = now
        var next: Vector3 = player.call("position")
        distance += position.distance_to(next)
        route.update(next, elapsed)
        position = next
        gpu.append(RenderingServer.viewport_get_measured_render_time_gpu(viewport))
        render_cpu.append(RenderingServer.viewport_get_measured_render_time_cpu(viewport)+RenderingServer.get_frame_setup_time_cpu())
        process_cpu.append(Performance.get_monitor(Performance.TIME_PROCESS)*1000.0)
        wall.append(elapsed*1000.0)
    key.pressed = false
    Input.parse_input_event(key)
    Input.flush_buffered_events()
    _check(gpu.size() >= 600 and gpu.min() > 0.0, label + " positive hardware GPU samples")
    var report := {"samples": gpu.size(), "duration_seconds": (Time.get_ticks_usec()-started)/1000000.0, "walk_skyrim_units": distance, "gpu_ms": _percentiles(gpu), "render_cpu_ms": _percentiles(render_cpu), "process_ms": _percentiles(process_cpu), "wall_frame_ms": _percentiles(wall), "resolution": [root.size.x, root.size.y]}
    measurements[label] = report
    print("R4 GPU ", label, " ", JSON.stringify(report))
    _check(report.gpu_ms.p99 < 16.67, label + " GPU p99 below 16.67 ms")
    _check(distance >= 100.0, label + " walks at least 100 Skyrim units")
    await RenderingServer.frame_post_draw
    root.get_texture().get_image().save_png(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join(label+"_gpu_walk.png"))

func _run() -> void:
    _check(OS.get_environment("DISPLAY") == ":99", "private display")
    _check(RenderingServer.get_video_adapter_vendor().to_lower().contains("nvidia"), "hardware NVIDIA Vulkan")
    if failures > 0:
        quit(1)
        return
    await super._start()
    if not loaded:
        quit(1)
        return
    measurements.adapter = RenderingServer.get_video_adapter_name()
    await _measure("inn")
    var exited := false
    for door: Dictionary in player.call("gameplay_doors"):
        if not door.locked:
            exited = player.call("activate_reference", door.file, door.object)
            break
    _check(exited, "inn exit")
    await _settle()
    await _measure("whiterun")
    var gate: Dictionary = player.call("detection_gate_reference")
    _check(not gate.is_empty(), "city gate")
    if not gate.is_empty():
        _check(player.call("activate_reference", gate.file, gate.object), "city gate exit")
        await _settle()
        await _measure("plains")
    var file := FileAccess.open(OS.get_environment("SOE_CHARACTER_OUTPUT").path_join("gpu_timings.json"), FileAccess.WRITE)
    file.store_string(JSON.stringify(measurements, "  "))
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " R4 private GPU walk")
    quit(0 if failures == 0 else 1)

func _process(_delta: float) -> bool:
    return false
