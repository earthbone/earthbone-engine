extends SceneTree

## RND-29 installed plains capture. Thresholds fixed before capture:
## zero LOD pixels inside loaded cells, zero sky pixels in the boundary ring,
## at least 1000 LOD pixels outside. Magenta identifies only LOD instances.
const CELL_METRES := 58.5216
const MIN_LOD_PIXELS := 1000
var failures := 0

func _initialize() -> void:
    call_deferred("rnd_29_terrain_join_capture")

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures += 1
        printerr("FAIL: ", message)

func _capture() -> Image:
    await process_frame
    await process_frame
    await RenderingServer.frame_post_draw
    return root.get_texture().get_image()

func rnd_29_terrain_join_capture() -> void:
    var data := OS.get_environment("SKYRIM_DATA")
    if data.is_empty():
        printerr("FAIL: SKYRIM_DATA is required")
        quit(2)
        return
    var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
    root.add_child(streamer)
    _check(streamer.call("load_data", data), "installed data load")
    var origin := {}
    var distance := 2147483647
    # CONTEXT: The plains south-west of Whiterun avoid the separate city world.
    for cell: Dictionary in streamer.call("editor_cell_tree"):
        if cell.get("interior", true) or String(cell.get("world_name", "")) != "Tamriel" or cell.name == cell.id:
            continue
        var d := maxi(absi(int(cell.x)), absi(int(cell.y) + 4))
        if d < distance:
            origin = cell
            distance = d
    _check(not origin.is_empty(), "named Tamriel plains cell near (0,-4)")
    if origin.is_empty():
        print(streamer.call("editor_cell_tree").slice(0, 3))
        streamer.free()
        quit(1)
        return
    var focus := Vector2i(int(origin.x), int(origin.y))
    print("RND-29 focus: ", origin.name, " ", focus)
    _check(streamer.call("stream_exterior", origin.name, 1) > 0, "loaded plains grid")
    _check(streamer.call("stream_terrain_lod", "Tamriel", focus.x, focus.y, 2, 12, 0) > 0, "LOD plains ring")
    var stats: Dictionary = streamer.call("terrain_lod_stats")
    var covered := {}
    for entry in String(stats.get("covered_cells", "")).split(" ", false):
        var pair := entry.split(":")
        covered[Vector2i(int(pair[0]), int(pair[1]))] = true
    _check(covered.size() == 9, "all nine loaded LAND cells mask LOD")
    var camera := Camera3D.new()
    camera.projection = Camera3D.PROJECTION_ORTHOGONAL
    camera.size = CELL_METRES * 6.0
    camera.far = 3000.0
    root.add_child(camera)
    var center := Vector3((focus.x + 0.5) * CELL_METRES, 0.0, -(focus.y + 0.5) * CELL_METRES)
    camera.position = center + Vector3(0, 1500, 0)
    camera.look_at(center, Vector3(0, 0, -1))
    camera.make_current()
    var environment := WorldEnvironment.new()
    var settings := Environment.new()
    settings.background_mode = Environment.BG_COLOR
    settings.background_color = Color(0, 1, 1)
    settings.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
    settings.ambient_light_color = Color.WHITE
    settings.ambient_light_energy = 0.7
    environment.environment = settings
    root.add_child(environment)
    var sun := DirectionalLight3D.new()
    sun.rotation_degrees = Vector3(-60, 25, 0)
    root.add_child(sun)
    var image: Image = await _capture()
    _check(image.save_png("res://.godot/rnd-29-plains.png") == OK, "save normal plains capture")
    streamer.call("set_terrain_lod_debug_color", true)
    var diagnostic: Image = await _capture()
    _check(diagnostic.save_png("res://.godot/rnd-29-coverage.png") == OK, "save coverage capture")
    var overlap := 0
    var gap := 0
    var lod := 0
    for y in diagnostic.get_height():
        for x in diagnostic.get_width():
            var ray := camera.project_ray_origin(Vector2(x + 0.5, y + 0.5))
            var cell := Vector2i(floori(ray.x / CELL_METRES), floori(-ray.z / CELL_METRES))
            var pixel := diagnostic.get_pixel(x, y)
            var magenta := pixel.r > 0.8 and pixel.g < 0.1 and pixel.b > 0.8
            var sky := pixel.r < 0.1 and pixel.g > 0.8 and pixel.b > 0.8
            if magenta:
                if covered.has(cell):
                    overlap += 1
                else:
                    lod += 1
            if sky and absi(cell.x - focus.x) <= 2 and absi(cell.y - focus.y) <= 2:
                gap += 1
    _check(overlap == 0, "LOD pixels inside loaded cell grid: " + str(overlap))
    _check(gap == 0, "sky pixels in joined boundary ring: " + str(gap))
    _check(lod >= MIN_LOD_PIXELS, "insufficient visible LOD outside grid")
    print("RND-29 capture: overlap=", overlap, " gap=", gap, " outside_lod=", lod)
    streamer.call("set_terrain_lod_debug_color", false)
    var eye := OS.get_environment("SOE_RND29_EYE_HEIGHT")
    _check(not eye.is_empty(), "standing capture requires the recorded LAND height")
    camera.projection = Camera3D.PROJECTION_PERSPECTIVE
    camera.position = center + Vector3(0, eye.to_float(), 0)
    camera.look_at(camera.position + Vector3(0, -0.1, 1), Vector3.UP)
    var standing: Image = await _capture()
    _check(standing.save_png("res://.godot/rnd-29-standing-plains.png") == OK, "save standing plains capture")
    print("RND-29 standing: eye_height=", eye, " capture=res://.godot/rnd-29-standing-plains.png")
    camera.projection = Camera3D.PROJECTION_ORTHOGONAL
    camera.position = center + Vector3(0, 1500, 0)
    camera.look_at(center, Vector3(0, 0, -1))
    # CONTEXT: Replacing the near grid exercises restoration through the live host.
    streamer.call("clear")
    _check(streamer.call("stream_terrain_lod", "Tamriel", focus.x, focus.y, 2, 12, 0) > 0, "unloaded grid retains LOD")
    _check(String(streamer.call("terrain_lod_stats").get("covered_cells", "")).is_empty(), "unloaded LAND remains masked")
    streamer.call("set_terrain_lod_debug_color", true)
    var restored: Image = await _capture()
    var restored_pixels := 0
    for y in restored.get_height():
        for x in restored.get_width():
            var ray := camera.project_ray_origin(Vector2(x + 0.5, y + 0.5))
            var cell := Vector2i(floori(ray.x / CELL_METRES), floori(-ray.z / CELL_METRES))
            var pixel := restored.get_pixel(x, y)
            if covered.has(cell) and pixel.r > 0.8 and pixel.g < 0.1 and pixel.b > 0.8:
                restored_pixels += 1
    _check(restored_pixels >= MIN_LOD_PIXELS, "unloaded cells did not regain visible LOD")
    print("RND-29 restoration: pixels=", restored_pixels)
    streamer.free()
    if failures == 0:
        print("RESULT: PASS: RND-29 terrain joins loaded cells without overlap or gap")
    quit(0 if failures == 0 else 1)
