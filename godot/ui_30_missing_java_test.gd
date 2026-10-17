# SPDX-License-Identifier: GPL-3.0-or-later
extends SceneTree
## Exercise the production fallback and its visible tool diagnostic.

func _initialize() -> void:
	create_timer(60).timeout.connect(func(): quit(2))
	call_deferred("run")

func run() -> void:
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	assert(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")))
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	assert(player.call("use_streamer_data", streamer))
	assert(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0)
	assert(player.call("start_gameplay"))
	assert(player.call("open_inventory_menu"))
	await process_frame
	await process_frame
	var notice: Label = player.get_node("InterfaceNotices/InterfaceNotice")
	assert(notice.is_visible_in_tree())
	assert(notice.text.contains("Java unavailable"))
	assert(notice.get_parent().layer == 200)
	await RenderingServer.frame_post_draw
	var pixels := root.get_texture().get_image()
	assert(pixels.save_png(OS.get_environment("SOE_UI_30_NOTICE_CAPTURE")) == OK)
	var rect := Rect2i(notice.get_global_rect()).intersection(Rect2i(Vector2i.ZERO, pixels.get_size()))
	var ink := 0
	for y in range(rect.position.y, rect.end.y):
		for x in range(rect.position.x, rect.end.x):
			if pixels.get_pixel(x, y).get_luminance() > 0.9:
				ink += 1
	assert(ink > 24)
	print("UI-30 NOTICE PASS: ", notice.text)
	player.call("close_menu")
	player.free()
	streamer.free()
	await process_frame
	quit(0)
