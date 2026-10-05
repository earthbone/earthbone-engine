extends "res://walkthrough.gd"

const LightingChecks := preload("res://rnd_21_lighting_checks.gd")
var checks := LightingChecks.new()
var checking := false

func _process(_delta: float) -> bool:
	if is_instance_valid(player):
		player.set("auto_step", false)
	if loaded and not checking:
		checking = true
		call_deferred("_check_lighting")
	return false

func _check_lighting() -> void:
	var sky := checks.controller(root)
	if sky == null:
		quit(1)
		return
	sky.set_process(false)
	var streamer := root.find_children("*", "CellStreamerNode", true, false)[0]
	var expected: Dictionary = streamer.call("cell_lighting", "WhiterunBanneredMare")
	checks.interior(sky, expected)
	checks.check(sky.last_frame.weather_sky and sky.last_frame.sky_visible, "Bannered Mare Show Sky does not select a weather sun")
	checks.check(player.call("enter_cell", "WhiterunOrigin", 0.0, 0.0, 500.0) > 0, "walkthrough changes to exterior")
	checks.exterior(sky)
	checks.check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "walkthrough returns to interior")
	checks.interior(sky, expected)
	checks.controller(root)
	for _frame in range(8):
		await process_frame
	await RenderingServer.frame_post_draw
	var path := ProjectSettings.globalize_path("res://../target/rnd_21_walkthrough.png")
	checks.check(root.get_texture().get_image().save_png(path) == OK, "capture production walkthrough")
	print("RND-21 WALKTHROUGH: ", "FAIL" if checks.failed else "PASS")
	quit(1 if checks.failed else 0)
