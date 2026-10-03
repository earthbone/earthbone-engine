extends SceneTree

const HEAD := "meshes/actors/character/character assets/femalehead.nif"


func _initialize() -> void:
	var extension = load("res://soe.gdextension")
	if extension == null:
		printerr("RESULT: FAIL: host extension resource did not load")
		quit(1)
		return
	var data_dir := OS.get_environment("SKYRIM_DATA")
	if data_dir == "":
		printerr("RESULT: FAIL: SKYRIM_DATA is not set")
		quit(1)
		return
	if not ClassDB.class_exists("SkinnedActorNode"):
		printerr("RESULT: FAIL: SkinnedActorNode is not registered")
		quit(1)
		return

	var actor = ClassDB.instantiate("SkinnedActorNode")
	if actor == null:
		printerr("RESULT: FAIL: SkinnedActorNode could not be instantiated")
		quit(1)
		return
	get_root().add_child(actor)
	if not actor.call("load_character", data_dir, HEAD):
		printerr("RESULT: FAIL: vanilla head did not load")
		quit(1)
		return

	var stats: Dictionary = actor.call("stats")
	if int(stats.get("expression_shapes", 0)) == 0:
		printerr("RESULT: FAIL: no expression blend shapes reached the host mesh")
		quit(1)
		return
	print("RESULT: PASS: %d expression blend shapes uploaded" % stats["expression_shapes"])
	actor.free()
	await RenderingServer.frame_post_draw
	await RenderingServer.frame_post_draw
	quit(0)
