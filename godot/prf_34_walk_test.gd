extends "res://walkthrough.gd"

## PRF-34 fixed limits, set before the first baseline walk.
const SYNC_LIMIT_MS := 0.2
const SCENE_P50_MS := 0.5
const SCENE_P99_MS := 1.0
const WALK_SECONDS := 60.0
var samples: Dictionary = {}
var walk_started := 0
var location := 0
var failures := 0
var active := false
var source: Node3D
var ignored_frames := 2
var moving_forward := true
var sky_view: Node3D


func _initialize() -> void:
    call_deferred("_run")


func _run() -> void:
    await super._start()
    if not loaded:
        quit(1)
        return
    source = player.get_parent().get_child(0)
    for child in player.get_parent().get_children():
        if child.get_script() == RecordSky:
            sky_view = child
    player.set("auto_step", true)
    player.set("capture_mouse", false)
    _begin_walk()


func _begin_walk() -> void:
    samples.clear()
    ignored_frames = 2
    walk_started = Time.get_ticks_usec()
    active = true
    moving_forward = true
    _key(true, KEY_W)


func _key(pressed: bool, code: int) -> void:
    var event := InputEventKey.new()
    event.keycode = code
    event.physical_keycode = code
    event.pressed = pressed
    Input.parse_input_event(event)
    Input.flush_buffered_events()


func _process(_delta: float) -> bool:
    if not active:
        return false
    var seconds := float(Time.get_ticks_usec() - walk_started) / 1e6
    var forward := int(seconds / 2.0) % 2 == 0
    if forward != moving_forward:
        moving_forward = forward
        _key(forward, KEY_W)
        _key(not forward, KEY_S)
    var report: Dictionary = player.call("system_frame_breakdown")
    if not report.has("host_scene_cpu"):
        var cpu := 0.0
        for phase in ["reference_collision_sync", "reference_change_sync", "node_transforms", "mesh_material_uploads", "host_scene_other"]:
            cpu += report.get(phase, {}).get("ms", 0.0)
        report.host_scene_cpu = {"ms": cpu}
    if ignored_frames > 0:
        ignored_frames -= 1
        walk_started = Time.get_ticks_usec()
        return false
    for phase in report:
        var ms: float = report[phase].ms
        if not samples.has(phase):
            samples[phase] = []
        samples[phase].append(ms)
        if phase in ["reference_collision_sync", "reference_change_sync"] and ms > SYNC_LIMIT_MS:
            failures += 1
    if seconds >= WALK_SECONDS:
        active = false
        _key(false, KEY_W)
        _key(false, KEY_S)
        _finish_walk()
    return false


func _finish_walk() -> void:
    var label := "Bannered Mare" if location == 0 else "Whiterun plains"
    print("PRF-34 ", label, " frames=", samples.host_scene_cpu.size(), " seconds=", WALK_SECONDS)
    for phase in samples:
        var values: Array = samples[phase]
        values.sort()
        var p50: float = values[int(ceil(values.size() * 0.5)) - 1]
        var p99: float = values[int(ceil(values.size() * 0.99)) - 1]
        print("PRF-34 ", phase, " p50_ms=", p50, " p99_ms=", p99, " worst_ms=", values.back())
        if phase == "host_scene_cpu" and (p50 > SCENE_P50_MS or p99 > SCENE_P99_MS):
            failures += 1
    if location == 1:
        print("prf_34_sixty_second_walk_budgets failures=", failures)
        quit(0 if failures == 0 else 1)
        return
    location = 1
    if is_instance_valid(sky_view):
        sky_view.set_process(false)
    player.free()
    player = ClassDB.instantiate("PlayerNode")
    player.set("capture_mouse", false)
    player.set("auto_step", true)
    root.add_child(player)
    player.call("use_streamer_data", source)
    var spawn := Vector3.ZERO
    var found := false
    for cell: Dictionary in source.call("editor_cell_tree"):
        if cell.name == "WhiterunExterior04" and cell.world_name == "Tamriel":
            spawn = Vector3(float(cell.x) * 4096.0 + 2048.0, float(cell.y) * 4096.0 + 2048.0, 4096.0)
            found = true
            break
    if not found or player.call("enter_cell", "WhiterunExterior04", spawn.x, spawn.y, spawn.z) <= 0 or not player.call("start_gameplay"):
        printerr("PRF-34 plains entry failed")
        quit(1)
        return
    system_overlay.player = player
    if is_instance_valid(sky_view):
        sky_view.cell = "WhiterunExterior04"
        sky_view.use_player(player)
        sky_view.set_process(true)
    _begin_walk()
