extends "res://r4_runtime_test.gd"

## Isolate first-use full-tree material pipelines in the real free-walk scene.
const TREE_MODELS := [
	"landscape/trees/treepineforest01.nif",
	"landscape/trees/treeaspen01.nif",
]


func _sample(label: String, fps: int) -> void:
	await super._sample(label, fps)
	if label != "door_walk_60fps":
		return
	var settled: Dictionary = await Warmup.prepare()
	_check(String(settled.error).is_empty(), "scene pipelines settle before the tree probe")
	player.call("set_detailed_tracing", true)
	var trees: Array[Dictionary] = []
	for model: String in TREE_MODELS:
		var before := Warmup.snapshot()
		var started := Time.get_ticks_usec()
		var meshes: Array = streamer.call("model_meshes", model)
		var build_ms := (Time.get_ticks_usec() - started) / 1000.0
		_check(not meshes.is_empty(), "full tree model loaded: " + model)
		var rids: Array[RID] = []
		var vertices := 0
		var feet: Vector3 = player.call("position")
		var position := Vector3(feet.x / 70.0 + 12, feet.z / 70.0, -feet.y / 70.0)
		for mesh: ArrayMesh in meshes:
			vertices += mesh.surface_get_array_len(0)
			var rid := RenderingServer.instance_create2(mesh.get_rid(), streamer.get_world_3d().get_scenario())
			RenderingServer.instance_set_transform(rid, Transform3D(Basis.IDENTITY, position))
			rids.append(rid)
		for _frame in 3:
			await RenderingServer.frame_post_draw
		var counts := Warmup.snapshot()
		for index in counts.size():
			counts[index] -= before[index]
		var pipelines := Warmup.named(counts)
		var row := {"model": model, "meshes": meshes.size(), "vertices": vertices, "prepare_upload_ms": build_ms, "pipelines": pipelines}
		trees.append(row)
		print("R4_FULL_TREE ", JSON.stringify(row))
		_check(vertices > 100, "full tree has authored geometry")
		_check(int(pipelines.mesh) == 0 and int(pipelines.surface) == 0 and int(pipelines.draw) == 0, "loading a full tree adds no frame-path pipeline compilations")
		for rid in rids:
			RenderingServer.free_rid(rid)
	results["full_trees"] = trees
	player.call("set_detailed_tracing", false)
