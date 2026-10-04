extends SceneTree

## RND-17: the installed Whiterun battle barricade is built only after its
## disabled enable marker is enabled, then hides rendering and collision.
## Run the gated Rust rnd_17_enable_streaming test to write the form manifest.

var failures := 0

func _initialize() -> void:
    call_deferred("_run")

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL rnd_17: ", message)

func _run() -> void:
    var data := OS.get_environment("SKYRIM_DATA")
    if data.is_empty():
        print("SKIP rnd_17: SKYRIM_DATA unset")
        quit(0)
        return
    if not ClassDB.class_exists("CellStreamerNode"):
        printerr("FAIL rnd_17: extension missing")
        quit(1)
        return
    var target := OS.get_environment("CARGO_TARGET_DIR")
    if target.is_empty():
        target = ProjectSettings.globalize_path("res://../target")
    var manifest = JSON.parse_string(FileAccess.get_file_as_string(target.path_join("rnd_17_whiterun.json")))
    if not manifest is Dictionary:
        printerr("FAIL rnd_17: run the gated Rust test to generate the manifest")
        quit(1)
        return
    var file := int(manifest.file)
    var reference := int(manifest.reference)
    var marker := int(manifest.marker)
    var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
    get_root().add_child(streamer)
    _check(streamer.call("load_data", data), "data load")
    _check(streamer.call("stream_exterior", manifest.cell, 0) > 0, "WhiterunWorld streams")
    _check(streamer.call("last_stats").get("world", "") == "WhiterunWorld", "streamed cell belongs to WhiterunWorld")
    _check(streamer.call("stream_cell_collision", manifest.cell) > 0, "cell collision streams")
    _check(streamer.call("rendered_reference_count", file, reference) == 0, "disabled barricade has no uploaded geometry")
    _check(streamer.call("reference_collision_bodies", file, reference) == 0, "disabled barricade has no host collision")
    var player: Node3D = ClassDB.instantiate("PlayerNode")
    player.set("capture_mouse", false)
    player.set("auto_step", false)
    get_root().add_child(player)
    _check(player.call("use_streamer_data", streamer), "shared data")
    _check(player.call("enter_cell", manifest.cell, float(manifest.px), float(manifest.py), float(manifest.pz) + 1024.0) > 0, "player enters barricade cell")
    _check(player.call("start_gameplay"), "gameplay starts")
    _check(streamer.call("stream_cell_collision", manifest.cell) > 0, "optional streamer collision follows the gameplay location replacement")
    var children := streamer.get_child_count()
    player.call("advance", 0.0)
    _check(not player.call("gameplay_reference_pose", file, reference).get("visible", true), "runtime starts with barricade disabled")
    _check(streamer.call("rendered_reference_count", file, reference) == 0, "first tick still builds no barricade")
    _check(streamer.call("reference_collision_bodies", file, reference) == 0, "first tick has no barricade host collision")
    var initial_bodies: Dictionary = player.call("reference_collision_bodies", file, reference)
    _check(initial_bodies.get("movement", -1) == 0 and initial_bodies.get("query", -1) == 0, "initially disabled barricade has no player collision")
    for enabled in [true, false, true, false]:
        _check(player.call("set_reference_enabled", file, marker, enabled), "marker toggles")
        player.call("advance", 0.0)
        await process_frame
        _check(player.call("gameplay_reference_pose", file, reference).get("visible", not enabled) == enabled, "World derives child enable state")
        _check(streamer.call("rendered_reference_pose", file, reference).get("visible", not enabled) == enabled, "uploaded geometry visibility follows parent")
        var error: float = streamer.call("render_alignment_error", file, reference)
        _check(error >= 0.0 and error < 0.001, "actual uploaded vertices match the visible or collapsed placement")
        _check((streamer.call("reference_collision_bodies", file, reference) > 0) == enabled, "PhysicsServer collision follows parent")
        var bodies: Dictionary = player.call("reference_collision_bodies", file, reference)
        _check((int(bodies.get("movement", -1)) > 0) == enabled, "movement collision follows parent")
        _check((int(bodies.get("query", -1)) > 0) == enabled, "query collision follows parent")
        _check(streamer.get_child_count() == children, "enable adds no scene nodes")
        print("rnd_17 enabled=", enabled, " alignment=", error, " bodies=", bodies)
    player.free()
    streamer.free()
    print("RESULT: ", "PASS" if failures == 0 else "FAIL", " rnd_17_whiterun_runtime_enable_without_reload")
    quit(0 if failures == 0 else 1)
