extends "res://walkthrough.gd"

## Real Bannered Mare NIF bodies, isolated from tables after free walk startup.
const STEP := 1.0 / 120.0
var checking := false
var failures := 0

func _initialize() -> void:
    Engine.physics_ticks_per_second = 120
    _watchdog()
    super._initialize()

func _configure_player() -> void:
    player.set("capture_mouse", false)

func _watchdog() -> void:
    await create_timer(240.0).timeout
    printerr("FAIL round4_carry: watchdog")
    quit(1)

func _process(delta: float) -> bool:
    if loaded and not checking:
        checking = true
        call_deferred("_measure")
    return super._process(delta)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL round4_carry: ", message)

func _center(body: RID, anchor: Vector3) -> Vector3:
    var pose: Transform3D = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM)
    return pose * anchor

func _reset(body: RID, anchor: Vector3) -> void:
    PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM, Transform3D(Basis.IDENTITY, Vector3(0, 100, 0) - anchor))
    PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_LINEAR_VELOCITY, Vector3.ZERO)
    PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_ANGULAR_VELOCITY, Vector3.ZERO)
    PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_SLEEPING, false)

func _pull(row: Dictionary, body: RID, anchor: Vector3, target: Vector3, count: int) -> void:
    for _step in range(count):
        await physics_frame
        _check(streamer.call("debug_pull_reference", row.file, row.object, anchor, target, STEP), "resident carry drive accepts body")

func _measure() -> void:
    player.set("auto_step", false)
    var references: Dictionary = JSON.parse_string(OS.get_environment("SOE_R4_MASS_REFERENCES"))
    var results := {}
    for family: String in ["cup", "basket", "heavy"]:
        var row: Dictionary = references[family]
        var body := RID()
        for pose: Dictionary in streamer.call("movable_reference_poses"):
            if pose.file == row.file and pose.object == row.object:
                body = pose.body
                break
        _check(body.is_valid(), family + " has its real resident body")
        if not body.is_valid():
            quit(1)
            return
        await physics_frame
        var state := PhysicsServer3D.body_get_direct_state(body)
        var anchor := state.center_of_mass_local
        var original: Transform3D = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM)
        var mass: float = PhysicsServer3D.body_get_param(body, PhysicsServer3D.BODY_PARAM_MASS)
        _reset(body, anchor)
        await _pull(row, body, anchor, Vector3(1, 100, 0), 30)
        var lag := absf(1.0 - _center(body, anchor).x)
        _reset(body, anchor)
        await _pull(row, body, anchor, Vector3(0, 100, 0), 240)
        var sag := 100.0 - _center(body, anchor).y
        var swing := 0.0
        if family == "basket":
            _reset(body, anchor)
            await _pull(row, body, anchor + Vector3(0.05, 0, 0), Vector3(0.05, 100, 0), 30)
            var angular: Vector3 = PhysicsServer3D.body_get_state(body, PhysicsServer3D.BODY_STATE_ANGULAR_VELOCITY)
            swing = angular.length()
            _check(swing > 0.1 and swing < 100.0, "edge grab produces bounded swing")
        results[family] = {"mass": mass, "lag": lag, "sag": sag, "swing": swing}
        PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM, original)
        PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_LINEAR_VELOCITY, Vector3.ZERO)
        PhysicsServer3D.body_set_state(body, PhysicsServer3D.BODY_STATE_ANGULAR_VELOCITY, Vector3.ZERO)
    print("round4_carry measurements ", JSON.stringify(results))
    _check(results.cup.lag < 0.5 and results.basket.lag > results.cup.lag * 1.25 and results.heavy.lag > results.basket.lag, "real bodies show increasing follow lag")
    _check(results.cup.sag < 0.06 and results.basket.sag > 0.12 and results.basket.sag < 0.20, "supported real bodies show mass-dependent sag")
    _check(results.heavy.sag > 1.0, "very heavy body cannot be supported by the force ceiling")
    print("PASS round4_carry" if failures == 0 else "FAIL round4_carry")
    quit(0 if failures == 0 else 1)
