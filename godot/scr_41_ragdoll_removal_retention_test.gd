extends SceneTree

## SCR-41 checks forced ragdoll removal across player collision recreation.

const BODY := "meshes/actors/character/character assets/malebody_1.nif"
var failures := 0


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("SKYRIM_DATA not set, skipping")
		quit(0)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "installed data failed to load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"initial collision failed")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	player.call("sync_contact_actors")
	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	get_root().add_child(visual)
	_check(visual.call("load_character", data, BODY), "human ragdoll rig failed to load")
	var selected: Dictionary = {}
	for row: Dictionary in player.call("contact_actor_bodies"):
		if player.call("bind_actor_appearance", visual, row.file, row.object):
			selected = row
			break
	_check(not selected.is_empty(), "no resident actor accepted an appearance")
	if selected.is_empty():
		quit(1)
		return
	_check(player.call("force_actor_ragdoll", selected.file, selected.object, true),
		"forced ragdoll state update failed")
	for _frame in 5:
		await process_frame
	_check(visual.call("ragdoll_bodies").size() == 18, "forced ragdoll did not install its Jolt bodies")
	_check(player.call("force_actor_ragdoll", selected.file, selected.object, false),
		"forced ragdoll removal failed")
	for _frame in 5:
		await process_frame
	_check(visual.call("ragdoll_bodies").is_empty(), "forced removal left Jolt bodies active")
	_check(visual.call("render_mode") == "animated", "forced removal did not restore animated rendering")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"collision rebuild failed")
	for _frame in 5:
		await process_frame
	_check(visual.call("ragdoll_bodies").is_empty(), "collision rebuild restored removed ragdoll bodies")
	_check(visual.call("render_mode") == "animated", "collision rebuild restored removed ragdoll mode")
	visual.queue_free()
	player.queue_free()
	streamer.queue_free()
	await process_frame
	print("SCR-41 ", "PASS" if failures == 0 else "FAIL", " ragdoll removal retention")
	quit(0 if failures == 0 else 1)
