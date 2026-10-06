extends SceneTree

## Installed-data contract for PRF-08's mesh material binding. Headless safe.


func _initialize() -> void:
	call_deferred("_check")


func _check() -> void:
	var scene := Node3D.new()
	get_root().add_child(scene)
	var streamer := ClassDB.instantiate("CellStreamerNode") as Node3D
	if streamer == null:
		printerr("RESULT: FAIL: extension did not register CellStreamerNode")
		quit(1)
		return
	scene.add_child(streamer)
	if not streamer.call("load_data", OS.get_environment("SKYRIM_DATA")):
		printerr("RESULT: FAIL: could not load installed data")
		quit(1)
		return
	for batch in [true, false]:
		streamer.set("batch_surfaces", batch)
		var built: int = streamer.call("stream_exterior", "WhiterunOrigin", 1)
		var stats: Dictionary = streamer.call("last_stats")
		if built <= 0 or stats.cells != 9 or stats.meshes_failed != 0:
			printerr("RESULT: FAIL: Whiterun radius 1 did not build nine cells")
			quit(1)
			return
		if streamer.call("surface_material_count") != built:
			printerr("RESULT: FAIL: a streamed mesh has no material before its first frame")
			quit(1)
			return
		print("RESULT: PASS: prf_08 batch=%s %d meshes have materials before the first frame" % [batch, built])
		streamer.call("clear")
		if streamer.call("surface_material_count") != 0:
			printerr("RESULT: FAIL: clearing retained meshes")
			quit(1)
			return
	quit(0)
