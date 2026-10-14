# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://tst_19_play_path.gd"

## Exercise the real walker and observer scheduler with slow captures and no assets.
class FixturePlayer extends Node3D:
    var feet := Vector3.ZERO
    func position() -> Vector3:
        return feet
    func _process(delta: float) -> void:
        if Input.is_physical_key_pressed(KEY_W):
            feet.y += 100.0 * delta
    func set_facing(_yaw: float, _pitch: float) -> void:
        pass
    func play_path_ground(_point: Vector3) -> Dictionary:
        return {}
    func play_path_navigation_details(target: Vector3) -> Dictionary:
        return {"points": PackedVector3Array([feet, target]), "reason": ""}
    func npc_visuals() -> Array:
        return []
    func npc_grounding_status() -> Array:
        return []
    func is_menu_open() -> bool:
        return false

var samples := 0
var walked := false
var returned := false

func _initialize() -> void:
    Engine.max_fps = 60
    player = FixturePlayer.new()
    root.add_child(player)
    var camera := Camera3D.new()
    root.add_child(camera)
    camera.current = true
    footstep_capture = AudioEffectCapture.new()
    loaded = true
    duration_s = 3.0
    route_start = Time.get_ticks_usec()
    previous_frame = route_start
    next_sample = route_start
    call_deferred("_walk_fixture")
    _fixture_watchdog()

func _walk_fixture() -> void:
    walked = await _walk_to(Vector3(0, 100, 0), 5.0)
    if walked:
        completed_loops = 1
        walked = await _walk_to(Vector3(0, 300, 0), 5.0)
    returned = true
    await _stop("fixture")

func _npcs(_camera: Camera3D) -> void:
    samples += 1
    await create_timer(1.1).timeout

func _drain_audio() -> void:
    pass

func _surface_checks(_camera: Camera3D) -> void:
    pass

func _check(_id: String, _passed: bool, _value: Dictionary) -> void:
    pass

func _finish() -> void:
    finished = true
    _key(false)
    var ok := returned and walked and samples > 0 and _elapsed() >= 3.0 and _elapsed() < 5.0
    print("TST19_SCHEDULER ", "PASS" if ok else "FAIL", " returned=", returned, " walked=", walked, " samples=", samples, " elapsed_s=", _elapsed())
    quit(0 if ok else 1)

func _fixture_watchdog() -> void:
    await create_timer(8.0).timeout
    if not finished:
        printerr("TST19_SCHEDULER FAIL: slow observation starved walker or completion")
        quit(1)
