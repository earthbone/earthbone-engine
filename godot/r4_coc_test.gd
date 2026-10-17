extends "res://walkthrough.gd"

var failures: Array[String] = []

func _configure_player() -> void:
    root.size = Vector2i(640, 360)
    player.set("capture_mouse", false)

func _spawn_position(_source: Node3D) -> Vector3:
    return Vector3(-125, -800, 120)

func _check(ok: bool, message: String) -> void:
    if not ok:
        failures.append(message)
        printerr("R4_COC failure: ", message)

func _start() -> void:
    await super._start()
    if not loaded:
        return
    player.set("auto_step", false)
    player.call("console_set_open", true)
    var sky: Node
    for child in player.get_parent().get_children():
        if child.get_script() == RecordSky:
            sky = child
    _check(sky != null, "walkthrough has its production sky controller")
    for cell_name in ["QASmoke", "WarehouseFurniture", "XTestGrantInt", "WhiterunExterior04", "WhiterunBanneredMare"]:
        var result := str(player.call("console_execute", "coc " + cell_name))
        _check(not result.contains("error:"), cell_name + " command succeeds: " + result)
        var camera := root.get_camera_3d()
        var before_image := _image_values(camera)
        player.call("select_weather_image_space", "", 1, 1, 0.0)
        var after_image := _image_values(camera)
        _check(before_image == after_image, cell_name + " replaces imagespace while console pauses gameplay: before=" + str(before_image) + " expected=" + str(after_image))
        await process_frame
        var position: Vector3 = player.call("position")
        _check(position.length() > 1.0, cell_name + " arrives away from cell origin")
        _check(absf(position.z) > 1.0, cell_name + " uses authored height")
        if sky != null:
            _check(sky.refresh(), cell_name + " refreshes sky")
            var frame: Dictionary = sky.last_frame
            var expected: Dictionary = streamer.call("cell_lighting", cell_name)
            if not expected.is_empty():
                _check(frame.has("rotation"), cell_name + " applies interior lighting")
                _check(frame.get("ambient") == expected.ambient, cell_name + " applies resolved ambient")
                _check(frame.get("fog_color") == expected.fog_near_color, cell_name + " applies resolved near fog")
                _check(frame.get("fog_far_color") == expected.fog_far_color, cell_name + " applies resolved far fog")
                _check(is_equal_approx(frame.fog.x, expected.fog_near) and is_equal_approx(frame.fog.y, expected.fog_far), cell_name + " applies fog distances")
            else:
                _check(frame.get("weather_sky", false), cell_name + " replaces interior lighting with exterior weather")
            print("R4_COC_CELL ", cell_name, " position=", position, " frame=", frame)
        if cell_name == "QASmoke" and DisplayServer.get_name() != "headless":
            player.call("console_set_open", false)
            for frame_index in range(120):
                player.call("advance", 1.0 / 60.0)
                await process_frame
            await RenderingServer.frame_post_draw
            var capture := root.get_texture().get_image()
            var non_black := 0
            var sampled := 0
            var colors := {}
            for y in range(0, capture.get_height(), 8):
                for x in range(0, capture.get_width(), 8):
                    var pixel := capture.get_pixel(x, y)
                    sampled += 1
                    if maxf(pixel.r, maxf(pixel.g, pixel.b)) > 0.01:
                        non_black += 1
                    colors[pixel.to_html(false)] = true
            var visible_ratio := float(non_black) / float(sampled)
            var fade: ColorRect = player.get_node("DoorFadeLayer/DoorFade")
            print("R4_COC_CAPTURE non_black_ratio=", visible_ratio, " sampled_colors=", colors.size(), " position=", player.call("position"), " fade_visible=", fade.visible, " fade_alpha=", fade.color.a)
            _check(visible_ratio > 0.05 and colors.size() > 64, "qasmoke capture contains visible room detail")
            var capture_path := OS.get_environment("SOE_R4_COC_CAPTURE")
            if capture_path.is_empty():
                capture_path = "res://../target/r4-coc-qasmoke.png"
            _check(capture.save_png(capture_path) == OK, "qasmoke arrival capture saves")
            player.call("console_set_open", true)
    print("R4_COC_RESULT ", "PASS" if failures.is_empty() else "FAIL", " failures=", failures)
    quit(0 if failures.is_empty() else 1)

func _image_values(camera: Camera3D) -> Array:
    var environment := camera.environment
    var tint: ColorRect = player.get_node("ImageSpaceTintLayer/ImageSpaceTint")
    if environment == null:
        return [null, tint.visible, tint.color]
    return [environment.tonemap_exposure, environment.tonemap_white, environment.adjustment_saturation, environment.adjustment_brightness, environment.adjustment_contrast, tint.visible, tint.color]
