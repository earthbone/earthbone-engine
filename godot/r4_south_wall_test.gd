extends "res://walkthrough.gd"

var failures := 0

func _configure_player() -> void:
    player.set("capture_mouse", false)
    player.set("auto_step", false)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("R4_WALL FAIL: ", message)

func _spawn_position(source: Node3D) -> Vector3:
    var point := startup_position
    var terrain: Variant = source.call("play_path_terrain_height", Vector3(point.x * 0.0142875, 0, -point.y * 0.0142875))
    if terrain != null:
        point.z = float(terrain) + 120.0
    return point

func _start() -> void:
    startup_cell = "WhiterunExterior04"
    startup_radius = 1
    startup_position = Vector3(12000.0, -14000.0, -3000.0)
    await super._start()
    if not loaded:
        return
    player.set("auto_step", true)
    await process_frame
    await process_frame
    while player.call("is_location_preparing"):
        await process_frame
    player.set("auto_step", false)
    for _frame in range(120):
        player.call("advance", 1.0 / 60.0)
    await physics_frame
    var bodies: Dictionary = player.call("reference_collision_bodies", 0, 0xc8c1d)
    _check(bodies.movement > 0 and bodies.query > 0, "reported tree collision is resident")
    for x in [0.0, 4000.0, 8000.0, 12000.0]:
        var ray: Dictionary = player.call("reference_collision_ray", 0, 0xc8c1d,
            Vector3(x, -3000.0, 1000.0) * 0.0142875,
            Vector3(x, -3000.0, 12000.0) * 0.0142875)
        _check(not ray.movement and not ray.query, "no distant south wall at x=" + str(x))
    var before: Vector3 = player.call("position")
    player.call("set_facing", PI, 0.0)
    Input.action_press("Forward")
    for _frame in range(120):
        player.call("advance", 1.0 / 60.0)
    Input.action_release("Forward")
    var after: Vector3 = player.call("position")
    _check(before.y - after.y > 180.0, "walk south at least 180 units")
    _check(player.call("is_on_ground"), "south walk remains grounded")
    print("R4_WALL before=", before, " after=", after, " bodies=", bodies, " failures=", failures)
    print("RESULT: ", "PASS" if failures == 0 else "FAIL")
    quit(0 if failures == 0 else 1)
