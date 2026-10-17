extends "res://r4_streaming_test.gd"

## A billboard block must cover the band immediately beyond the near cells.
const MAX_MEDIUM_RING := 12


func _sample(label: String, fps: int) -> void:
	await super._sample(label, fps)
	if label != "door_walk_60fps":
		return
	var stats: Dictionary = streamer.call("tree_lod_stats")
	var position: Vector3 = player.call("position")
	var focus := Vector2i(floori(position.x / 4096.0), floori(position.y / 4096.0))
	var medium := 0
	for entry in String(stats.get("blocks", "")).split(" ", false):
		var values := entry.split(":")
		var span := int(values[0])
		var origin := Vector2i(int(values[1]), int(values[2]))
		if span == 1:
			var near: Dictionary = streamer.call("exterior_cell_batches", origin.x, origin.y)
			_check(int(near.instances) == 0, "drawn tree billboard cells have no near render batches")
		var nearest := Vector2i(clampi(focus.x, origin.x, origin.x + span - 1), clampi(focus.y, origin.y, origin.y + span - 1))
		var distance := maxi(absi(nearest.x - focus.x), absi(nearest.y - focus.y))
		if distance > 2 and distance <= MAX_MEDIUM_RING:
			medium += 1
	print("R4_TREE medium_blocks=", medium, " focus=", focus, " stats=", JSON.stringify(stats))
	_check(medium > 0, "tree billboards cover the medium band within 12 cells")
	var coverage: Dictionary = streamer.call("tree_lod_coverage", 8)
	print("R4_TREE coverage=", JSON.stringify(coverage))
	_check(not coverage.has("error"), "archived tree coverage survey completed")
	_check(int(coverage.get("expected", 0)) > 0, "archived trees exist outside the near cells")
	_check(int(coverage.get("missing", -1)) == 0, "every archived tree beyond near cells has a billboard")
	var report := OS.get_environment("SOE_R4_REPORT")
	if not report.is_empty():
		var survey := Camera3D.new()
		streamer.add_child(survey)
		survey.global_position = Vector3(position.x / 70.0, position.z / 70.0 + 40, -position.y / 70.0)
		survey.far = 2000.0
		survey.fov = 75.0
		survey.look_at(Vector3(-1000.0 / 70.0, 500.0 / 70.0, 30000.0 / 70.0))
		survey.make_current()
		for _frame in 3:
			await RenderingServer.frame_post_draw
		_check(get_root().get_texture().get_image().save_png(report.get_basename() + ".png") == OK, "tree view captured")
		survey.queue_free()
